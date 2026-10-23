//! Addition dispatch preserves equivalent union arguments and distinct generic arguments.
use crate::domain::{model::CompletionStatus, ports::ProjectStore, source::PythonVersion};
use crate::infrastructure::filesystem::Filesystem;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs;

#[test]
fn operator_semantic_equality_matches_fixed_original_on_all_targets() {
    let source = r###"from typing import Generic, TypeVar

T = TypeVar("T")

class G(Generic[T]):
    def __add__(self, other: str) -> int:
        return 1
    def __radd__(self, other: object) -> str:
        return "result"

def same(left: G[int | str], right: G[str | int]) -> str:
    return left + right

def different(left: G[int], right: G[str]) -> str:
    return left + right
"###;
    let original = r###"{"file": "main.py", "line": 12, "column": 11, "end_line": 12, "end_column": 23, "message": "Incompatible return value type (got \"int\", expected \"str\")", "hint": null, "code": "return-value", "severity": "error"}
{"file": "main.py", "line": 12, "column": 18, "end_line": 12, "end_column": 23, "message": "Unsupported operand types for + (\"G[int | str]\" and \"G[str | int]\")", "hint": null, "code": "operator", "severity": "error"}
"###;
    assert_eq!(
        format!("{:x}", Sha256::digest(source.as_bytes())),
        "d668925f8b691e19375136071c006b99e85a2ba7c444e713ccc5c58c729597e4"
    );
    assert_eq!(
        format!("{:x}", Sha256::digest(original.as_bytes())),
        "70f72f98b25891e89cfec1caf1af07cd658bec3f196c059e4e13004fbe896444"
    );
    let expected: Vec<Value> = original
        .lines()
        .map(|line| {
            let row: Value = serde_json::from_str(line).unwrap();
            json!({"rule": row["code"], "message": row["message"], "hint": row["hint"],
                "path": row["file"], "severity": row["severity"],
                "location": {"line": row["line"], "column": row["column"].as_i64().unwrap() + 1,
                    "end_line": row["end_line"], "end_column": row["end_column"]}})
        })
        .collect();
    for target in [
        PythonVersion::Py312,
        PythonVersion::Py313,
        PythonVersion::Py314,
    ] {
        let root = tempfile::tempdir().unwrap();
        let root_path = fs::canonicalize(root.path()).unwrap();
        fs::write(
            root_path.join("pyproject.toml"),
            "[project]\nname = \"fixture\"\nversion = \"0.0.0\"\nrequires-python = \">=3.12\"\n",
        )
        .unwrap();
        let path = root_path.join("main.py");
        fs::write(&path, source).unwrap();
        let mut workspace = Filesystem.load(std::slice::from_ref(&path), true);
        workspace.projects[0].python_version = target;
        let report = super::super::check(&workspace);
        assert_eq!(report.completion, CompletionStatus::Incomplete);
        assert_eq!(report.reference_exit_code, None);
        assert_eq!(report.checked_files, 1);
        let actual: Vec<_> = report.diagnostics.iter().map(|diagnostic| json!({
            "rule": diagnostic.rule, "message": diagnostic.message, "hint": diagnostic.hint,
            "path": diagnostic.path.strip_prefix(&root_path).unwrap(),
            "severity": diagnostic.severity,
            "location": {"line": diagnostic.location.line, "column": diagnostic.location.column,
                "end_line": diagnostic.location.end_line, "end_column": diagnostic.location.end_column},
        })).collect();
        assert_eq!(actual, expected, "{}", target.name());
        assert_eq!(fs::read(&path).unwrap(), source.as_bytes());
    }
}
