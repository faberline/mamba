//! Filter inference retains shadow controls and all original false-filter diagnostics.
use crate::domain::{model::CompletionStatus, ports::ProjectStore, source::PythonVersion};
use crate::infrastructure::filesystem::Filesystem;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs;

#[test]
fn comprehension_filter_matches_fixed_original_on_all_targets() {
    let source = r###"import ast

def narrowed(source: str) -> ast.Module:
    nodes = [
        node for node in ast.parse(source).body
        if isinstance(node, (ast.ClassDef, ast.FunctionDef)) and node.name == "target"
    ]
    return ast.Module(body=nodes, type_ignores=[])

def unfiltered(source: str) -> ast.Module:
    return ast.Module(body=ast.parse(source).body, type_ignores=[])

def shadowed(source: str) -> ast.Module:
    def isinstance(node: ast.stmt, classes: object) -> bool:
        return True
    nodes = [
        node for node in ast.parse(source).body
        if isinstance(node, (ast.ClassDef, ast.FunctionDef))
    ]
    return ast.Module(body=nodes, type_ignores=[])

def unreachable() -> list[str]:
    return [1 + "text" for _ in [0] if False]
"###;
    let original = r###"{"file": "main.py", "line": 8, "column": 27, "end_line": 8, "end_column": 32, "message": "Argument \"body\" to \"Module\" has incompatible type \"list[ClassDef | FunctionDef]\"; expected \"list[stmt]\"", "hint": "\"list\" is invariant -- see https://mypy.readthedocs.io/en/stable/common_issues.html#variance\nConsider using \"Sequence\" instead, which is covariant", "code": "arg-type", "severity": "error"}
{"file": "main.py", "line": 23, "column": 12, "end_line": 23, "end_column": 22, "message": "List comprehension has incompatible type List[int]; expected List[str]", "hint": null, "code": "misc", "severity": "error"}
{"file": "main.py", "line": 23, "column": 16, "end_line": 23, "end_column": 22, "message": "Unsupported operand types for + (\"int\" and \"str\")", "hint": null, "code": "operator", "severity": "error"}
"###;
    assert_eq!(
        format!("{:x}", Sha256::digest(source.as_bytes())),
        "2aba6544f20868ea67587e99d2eeb9c520e265260fa149e19efacd379bc96d4d"
    );
    assert_eq!(
        format!("{:x}", Sha256::digest(original.as_bytes())),
        "9cc5980a83be08542f271e98b5a36fdda3e736009e3b2a5a7c1bb6827c414461"
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
