//! Implicit methods keep their bare name while typed errors retain the class owner.
use crate::domain::{model::CompletionStatus, ports::ProjectStore, source::PythonVersion};
use crate::infrastructure::filesystem::Filesystem;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs;

#[test]
fn implicit_method_names_match_fixed_original_on_py312() {
    let source = r###"class ModuleHandler:
    def send(self, value):
        return value


def check() -> None:
    class LocalHandler:
        def send(self, value):
            return value

        def typed(self, value: int) -> None:
            pass

    ModuleHandler().send(1)
    LocalHandler().send(1)
    LocalHandler().typed("bad")
"###;
    let original = r###"{"file": "main.py", "line": 2, "column": 4, "end_line": 3, "end_column": 20, "message": "Function is missing a type annotation", "hint": null, "code": "no-untyped-def", "severity": "error"}
{"file": "main.py", "line": 8, "column": 8, "end_line": 9, "end_column": 24, "message": "Function is missing a type annotation", "hint": null, "code": "no-untyped-def", "severity": "error"}
{"file": "main.py", "line": 14, "column": 4, "end_line": 14, "end_column": 27, "message": "Call to untyped function \"send\" in typed context", "hint": null, "code": "no-untyped-call", "severity": "error"}
{"file": "main.py", "line": 15, "column": 4, "end_line": 15, "end_column": 26, "message": "Call to untyped function \"send\" in typed context", "hint": null, "code": "no-untyped-call", "severity": "error"}
{"file": "main.py", "line": 16, "column": 25, "end_line": 16, "end_column": 30, "message": "Argument 1 to \"typed\" of \"LocalHandler\" has incompatible type \"str\"; expected \"int\"", "hint": null, "code": "arg-type", "severity": "error"}
"###;
    assert_eq!(
        format!("{:x}", Sha256::digest(source.as_bytes())),
        "5bf689166cb222bb66506799aff65bf6605e007f518794700e99a4ec0b0ec78c"
    );
    assert_eq!(
        format!("{:x}", Sha256::digest(original.as_bytes())),
        "851ef4eae791c613739c95386e24bd59aa22b0c63a4c17c5ecc0c7bd6c280a2b"
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
