//! Environment mapping methods keep their real inherited string value constraints.
use crate::domain::{model::CompletionStatus, ports::ProjectStore, source::PythonVersion};
use crate::infrastructure::filesystem::Filesystem;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs;

#[test]
fn os_environ_methods_match_fixed_original_on_all_targets() {
    let source = r###"import os

def copy_environment() -> dict[str, str]:
    return dict(os.environ.items())

def update_environment() -> None:
    os.environ.update({"FIXTURE": "value"})
    os.environ.clear()

def invalid_value() -> None:
    os.environ.update({"FIXTURE": 1})
"###;
    let original = r###"{"file": "main.py", "line": 11, "column": 23, "end_line": 11, "end_column": 35, "message": "Dict entry 0 has incompatible type \"str\": \"int\"; expected \"str\": \"str\"", "hint": null, "code": "dict-item", "severity": "error"}
"###;
    assert_eq!(
        format!("{:x}", Sha256::digest(source.as_bytes())),
        "65540214e7490c6813f823633229422d7a254dfcfc5897a4ae4393c397a816d0"
    );
    assert_eq!(
        format!("{:x}", Sha256::digest(original.as_bytes())),
        "58077f93e3e8979e90918204724b6cf75c7a3ee905c37944fb10540026187c63"
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
