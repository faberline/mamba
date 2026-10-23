//! One fixed union and native/source identity regression retains original fields.
use crate::domain::{model::CompletionStatus, ports::ProjectStore, source::PythonVersion};
use crate::infrastructure::filesystem::Filesystem;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs;

#[test]
fn ast_union_and_native_identity_match_fixed_original_on_all_targets() {
    let source = r###"import ast

def union_ok(node: ast.Assign | ast.Expr) -> str:
    return ast.unparse(node)

def union_bad(node: ast.Assign | int) -> str:
    return ast.unparse(node)

def compile_ok(node: ast.Module) -> None:
    compile(node, "<ast>", "exec")
"###;
    let original = r###"{"file": "main.py", "line": 7, "column": 23, "end_line": 7, "end_column": 27, "message": "Argument 1 to \"unparse\" has incompatible type \"Assign | int\"; expected \"AST\"", "hint": null, "code": "arg-type", "severity": "error"}
"###;
    assert_eq!(
        format!("{:x}", Sha256::digest(source.as_bytes())),
        "4b17dc3e4998bbd6e32b2f36b8d405346ac53e23e397b8e47d1cf4d25935358d"
    );
    assert_eq!(
        format!("{:x}", Sha256::digest(original.as_bytes())),
        "73feb174b67d6464ab982d87fe3107a2e0633300d81799b6d4075d2cc213f9d5"
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
