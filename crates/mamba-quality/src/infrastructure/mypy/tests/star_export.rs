//! A package can explicitly reexport a star import without accepting missing members.
use crate::domain::{model::CompletionStatus, ports::ProjectStore, source::PythonVersion};
use crate::infrastructure::filesystem::Filesystem;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs;

#[test]
fn star_import_all_matches_fixed_original_on_py312() {
    let source = r###"import facade

facade.Client
facade.MambaMissingAttribute
"###;
    let original = r###"{"file": "main.py", "line": 4, "column": 0, "end_line": 4, "end_column": 28, "message": "Module has no attribute \"MambaMissingAttribute\"", "hint": null, "code": "attr-defined", "severity": "error"}
"###;
    assert_eq!(
        format!("{:x}", Sha256::digest(source.as_bytes())),
        "97ddffd467ca5f5b2a596741ca3887af249129529b07a504435fe6dd36f072c7"
    );
    assert_eq!(
        format!("{:x}", Sha256::digest(original.as_bytes())),
        "4b787d0cbb38aa323b89e61c21300c4b90b0d76acaa71c683d4fdb6d9822fdef"
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
    let package_files = [
        (
            "facade/__init__.py",
            r###"from .provider import *

__all__ = ["Client"]
"###,
        ),
        (
            "facade/provider.py",
            r###"__all__ = ["Client"]

class Client:
    pass
"###,
        ),
    ];
    for (name, contents) in package_files {
        let path = root_path.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
    }
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
    for (name, contents) in package_files {
        assert_eq!(fs::read(root_path.join(name)).unwrap(), contents.as_bytes());
    }
}
