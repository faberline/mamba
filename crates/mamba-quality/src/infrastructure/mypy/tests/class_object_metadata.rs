//! Class-object metadata preserves default, inherited and ClassVar store controls.
use crate::domain::{model::CompletionStatus, ports::ProjectStore, source::PythonVersion};
use crate::infrastructure::filesystem::Filesystem;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs;

#[test]
fn class_object_metadata_matches_fixed_original_on_all_targets() {
    let source = r###"import ast
from typing import ClassVar

def name_ok(node: ast.cmpop) -> str:
    return type(node).__name__

def name_bad(node: ast.cmpop) -> int:
    return type(node).__name__

def instance_bad(node: ast.cmpop) -> None:
    node.__name__

class Meta(type):
    tag: int = 1
    class_only: ClassVar[int] = 0

class Base(metaclass=Meta):
    pass

class Derived(Base):
    pass

def inherited_ok() -> int:
    return Derived.tag

def inherited_bad() -> str:
    return Derived.tag

def inherited_name_ok() -> str:
    return Derived.__name__

def classvar_bad() -> None:
    Meta.class_only = 1
    Derived.class_only = 1
"###;
    let original = r###"{"file": "main.py", "line": 8, "column": 11, "end_line": 8, "end_column": 30, "message": "Incompatible return value type (got \"str\", expected \"int\")", "hint": null, "code": "return-value", "severity": "error"}
{"file": "main.py", "line": 11, "column": 4, "end_line": 11, "end_column": 17, "message": "\"cmpop\" has no attribute \"__name__\"", "hint": null, "code": "attr-defined", "severity": "error"}
{"file": "main.py", "line": 27, "column": 11, "end_line": 27, "end_column": 22, "message": "Incompatible return value type (got \"int\", expected \"str\")", "hint": null, "code": "return-value", "severity": "error"}
{"file": "main.py", "line": 34, "column": 4, "end_line": 34, "end_column": 22, "message": "Cannot assign to class variable \"class_only\" via instance", "hint": null, "code": "misc", "severity": "error"}
"###;
    assert_eq!(
        format!("{:x}", Sha256::digest(source.as_bytes())),
        "39f70f2f3ddd14fb9ec424f800c55a754eca34d9c19a2c3036be6a699c3f9cfb"
    );
    assert_eq!(
        format!("{:x}", Sha256::digest(original.as_bytes())),
        "6dea1695a9796960845d4fd16dc091b4fe59dcc961b1b8a51ba67ef9d3d9d084"
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
