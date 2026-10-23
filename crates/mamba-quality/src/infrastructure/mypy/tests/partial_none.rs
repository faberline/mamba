//! Inferred None recovers while explicit annotations and late declarations retain their errors.
use crate::domain::{model::CompletionStatus, ports::ProjectStore, source::PythonVersion};
use crate::infrastructure::filesystem::Filesystem;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs;

#[test]
fn partial_none_matches_fixed_original_on_all_targets() {
    let source = r###"import ast

def untyped():
    return 1

def inferred(node: ast.cmpop, enabled: bool) -> None:
    comparison = None
    if enabled:
        comparison = type(node).__name__
    if comparison == "Eq":
        untyped()

def annotated(node: ast.cmpop) -> None:
    comparison: None = None
    comparison = type(node).__name__

def declared_later(node: ast.cmpop) -> None:
    comparison = None
    comparison: None = None
    comparison = type(node).__name__
"###;
    let original = r###"{"file": "main.py", "line": 3, "column": 0, "end_line": 4, "end_column": 12, "message": "Function is missing a return type annotation", "hint": null, "code": "no-untyped-def", "severity": "error"}
{"file": "main.py", "line": 11, "column": 8, "end_line": 11, "end_column": 17, "message": "Call to untyped function \"untyped\" in typed context", "hint": null, "code": "no-untyped-call", "severity": "error"}
{"file": "main.py", "line": 15, "column": 17, "end_line": 15, "end_column": 36, "message": "Incompatible types in assignment (expression has type \"str\", variable has type \"None\")", "hint": null, "code": "assignment", "severity": "error"}
{"file": "main.py", "line": 19, "column": 4, "end_line": 19, "end_column": 14, "message": "Name \"comparison\" already defined on line 18", "hint": null, "code": "no-redef", "severity": "error"}
"###;
    assert_eq!(
        format!("{:x}", Sha256::digest(source.as_bytes())),
        "ae10cfbf4d9b4e771026e348be252c2002c0930fc1e8303021c4405b76d1a776"
    );
    assert_eq!(
        format!("{:x}", Sha256::digest(original.as_bytes())),
        "2a00489db0dbbfeb2abd68aa1c63a99d648b88b058586613be0716d21044e147"
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
