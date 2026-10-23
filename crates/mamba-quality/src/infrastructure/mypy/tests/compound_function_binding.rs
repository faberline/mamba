//! Functions declared in a compound suite keep their lexical callable binding.
use crate::domain::{model::CompletionStatus, ports::ProjectStore, source::PythonVersion};
use crate::infrastructure::filesystem::Filesystem;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs;

#[test]
fn compound_functions_match_fixed_original_on_all_targets() {
    let source = r###"from contextlib import nullcontext

def run() -> None:
    with nullcontext():
        def request(value):
            return value
        request("x")
        def typed(value: int) -> int:
            return value
        reveal_type(typed(1))
"###;
    let original = r###"{"file": "main.py", "line": 5, "column": 8, "end_line": 6, "end_column": 24, "message": "Function is missing a type annotation", "hint": null, "code": "no-untyped-def", "severity": "error"}
{"file": "main.py", "line": 7, "column": 8, "end_line": 7, "end_column": 20, "message": "Call to untyped function \"request\" in typed context", "hint": null, "code": "no-untyped-call", "severity": "error"}
{"file": "main.py", "line": 10, "column": 20, "end_line": 10, "end_column": 28, "message": "Revealed type is \"int\"", "hint": null, "code": "misc", "severity": "note"}
"###;
    assert_eq!(
        format!("{:x}", Sha256::digest(source.as_bytes())),
        "234fd748024935943a75ec426c7d3e19829f7d6ae5ba74f097f42f8b572d2635"
    );
    assert_eq!(
        format!("{:x}", Sha256::digest(original.as_bytes())),
        "b0c0c3ccdee35d5790512d43a6548b0c28f39d80c9daa801365b1223c089a1a8"
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
