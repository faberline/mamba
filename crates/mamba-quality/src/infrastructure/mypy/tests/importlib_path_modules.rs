//! One fixed Path(SimplePath) call keeps each original target's literal rows.
use crate::domain::{model::CompletionStatus, ports::ProjectStore, source::PythonVersion};
use crate::infrastructure::filesystem::Filesystem;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs;

#[test]
fn importlib_path_modules_match_fixed_original_on_all_targets() {
    let source = r###"from importlib.metadata import distributions
from pathlib import Path

def bad() -> None:
    for distribution in distributions():
        Path(distribution.locate_file("item"))

def good(value: str) -> Path:
    return Path(value)
"###;
    assert_eq!(
        format!("{:x}", Sha256::digest(source.as_bytes())),
        "d886a289ae3fd6eb44470c23a4b83f890d856690997cdecbc91e239f28081532"
    );
    for (target, original, original_sha) in [
        (
            PythonVersion::Py312,
            r###"{"file": "main.py", "line": 6, "column": 13, "end_line": 6, "end_column": 45, "message": "Argument 1 to \"Path\" has incompatible type \"SimplePath[Any]\"; expected \"str | PathLike[str]\"", "hint": null, "code": "arg-type", "severity": "error"}
"###,
            "7600be6c83a0e3ffa7ecb59946aaacdddaf2a8ffbaf29669852c6a075572dc76",
        ),
        (
            PythonVersion::Py313,
            r###"{"file": "main.py", "line": 6, "column": 13, "end_line": 6, "end_column": 45, "message": "Argument 1 to \"Path\" has incompatible type \"SimplePath\"; expected \"str | PathLike[str]\"", "hint": null, "code": "arg-type", "severity": "error"}
"###,
            "09ee65a6adfb306d8c2f306b2d0e3ac72aa81eb4adba87291398eb08a851ea07",
        ),
        (
            PythonVersion::Py314,
            r###"{"file": "main.py", "line": 6, "column": 13, "end_line": 6, "end_column": 45, "message": "Argument 1 to \"Path\" has incompatible type \"SimplePath\"; expected \"str | PathLike[str]\"", "hint": null, "code": "arg-type", "severity": "error"}
"###,
            "09ee65a6adfb306d8c2f306b2d0e3ac72aa81eb4adba87291398eb08a851ea07",
        ),
    ] {
        assert_eq!(
            format!("{:x}", Sha256::digest(original.as_bytes())),
            original_sha
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
