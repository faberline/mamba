//! Callable Union members retain Any returns and each concrete call constraint.
use crate::domain::{model::CompletionStatus, ports::ProjectStore, source::PythonVersion};
use crate::infrastructure::filesystem::Filesystem;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs;

#[test]
fn union_callable_any_matches_fixed_original_on_all_targets() {
    let source = r###"from typing import Any, Callable

def check(callback: Callable[[int], str] | Any,
          optional: Callable[[], int] | None,
          mixed: Callable[[], int] | Any | int) -> None:
    reveal_type(callback(1))
    callback("bad")
    optional()
    mixed()
"###;
    let original = r###"{"file": "main.py", "line": 6, "column": 16, "end_line": 6, "end_column": 27, "message": "Revealed type is \"str | Any\"", "hint": null, "code": "misc", "severity": "note"}
{"file": "main.py", "line": 7, "column": 13, "end_line": 7, "end_column": 18, "message": "Argument 1 has incompatible type \"str\"; expected \"int\"", "hint": null, "code": "arg-type", "severity": "error"}
{"file": "main.py", "line": 8, "column": 4, "end_line": 8, "end_column": 14, "message": "\"None\" not callable", "hint": null, "code": "misc", "severity": "error"}
{"file": "main.py", "line": 9, "column": 4, "end_line": 9, "end_column": 11, "message": "\"int\" not callable", "hint": null, "code": "operator", "severity": "error"}
"###;
    assert_eq!(
        format!("{:x}", Sha256::digest(source.as_bytes())),
        "c14662c6c9f2f20fd4b092fe545e9eb028adf95a7c7ad329f23ec8b231b90d6f"
    );
    assert_eq!(
        format!("{:x}", Sha256::digest(original.as_bytes())),
        "1727a081cb26f4319ae7eb4fa931cea17359e0fc7b577ce08d1b254d32f38ede"
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
