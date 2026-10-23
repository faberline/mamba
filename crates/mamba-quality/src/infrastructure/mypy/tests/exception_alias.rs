//! Exception aliases retain their handler type and become deleted after the handler.
use crate::domain::{model::CompletionStatus, ports::ProjectStore, source::PythonVersion};
use crate::infrastructure::filesystem::Filesystem;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs;

#[test]
fn except_alias_matches_fixed_original_on_py312() {
    let source = r###"def handle(value: str) -> None:
    try:
        int(value)
    except (ValueError, RecursionError) as error:
        reveal_type(error)
        raise RuntimeError("invalid") from error
    print(error)

def control() -> None:
    raise RuntimeError("invalid") from error
"###;
    let original = r###"{"file": "main.py", "line": 5, "column": 20, "end_line": 5, "end_column": 25, "message": "Revealed type is \"ValueError | RecursionError\"", "hint": null, "code": "misc", "severity": "note"}
{"file": "main.py", "line": 7, "column": 10, "end_line": 7, "end_column": 15, "message": "Trying to read deleted variable \"error\"", "hint": null, "code": "misc", "severity": "error"}
{"file": "main.py", "line": 10, "column": 39, "end_line": 10, "end_column": 44, "message": "Name \"error\" is not defined", "hint": null, "code": "name-defined", "severity": "error"}
"###;
    assert_eq!(
        format!("{:x}", Sha256::digest(source.as_bytes())),
        "651c818be7ae207fbee4df0b61df6fa0c4bd122a8f8ca79c22c4a1389fd624ed"
    );
    assert_eq!(
        format!("{:x}", Sha256::digest(original.as_bytes())),
        "46f5f692dc7c170aaa7589677f654946a5ac87408321ae0951cbe9910315b6b8"
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
    workspace.projects[0].python_version = PythonVersion::Py312;
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
    assert_eq!(actual, expected);
    assert_eq!(fs::read(&path).unwrap(), source.as_bytes());
}
