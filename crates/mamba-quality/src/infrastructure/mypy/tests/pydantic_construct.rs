//! A class receiver retains its Pydantic model and generic field types.
use super::{catalog, CompletionStatus, Filesystem, ProjectStore, PythonVersion};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs;

#[test]
fn generic_model_construct_preserves_receiver_and_argument_errors() {
    let selected = [
        "pydanticconstruct1_construct_0",
        "pydanticconstruct1_construct_1",
        "pydanticconstruct1_construct_12",
    ];
    let mut seen = std::collections::BTreeSet::new();
    for case in catalog::CHUNKS
        .iter()
        .flat_map(|chunk| serde_json::from_str::<Vec<Value>>(chunk).unwrap())
        .filter(|case| selected.contains(&case["name"].as_str().unwrap()))
    {
        assert!(seen.insert(case["name"].as_str().unwrap().to_owned()));
        let source = case["source"].as_str().unwrap();
        assert_eq!(
            format!("{:x}", Sha256::digest(source.as_bytes())),
            case["source_sha256"].as_str().unwrap()
        );
        let root = tempfile::tempdir().unwrap();
        let root_path = fs::canonicalize(root.path()).unwrap();
        let path = root_path.join(case["filename"].as_str().unwrap());
        fs::write(&path, source).unwrap();
        let mut workspace = Filesystem.load(std::slice::from_ref(&path), true);
        for (target, version) in [
            PythonVersion::Py312,
            PythonVersion::Py313,
            PythonVersion::Py314,
        ]
        .into_iter()
        .enumerate()
        {
            workspace.projects[0].python_version = version;
            let report = super::super::check(&workspace);
            assert_eq!(report.completion, CompletionStatus::Incomplete);
            assert_eq!(report.reference_exit_code, None);
            assert_eq!(report.checked_files, 1);
            let actual: Vec<_> = report.diagnostics.iter().map(|diagnostic| json!({
                "rule": if diagnostic.rule.is_empty() { Value::Null } else { Value::String(diagnostic.rule.clone()) },
                "message": diagnostic.message, "hint": diagnostic.hint,
                "path": diagnostic.path.strip_prefix(&root_path).unwrap(),
                "severity": diagnostic.severity,
                "location": {"line": diagnostic.location.line, "column": diagnostic.location.column,
                    "end_line": diagnostic.location.end_line, "end_column": diagnostic.location.end_column},
            })).collect();
            assert_eq!(
                Value::Array(actual),
                case["expected"][target],
                "{} / {}",
                case["name"],
                version.name()
            );
        }
    }
    assert_eq!(seen.len(), selected.len());
}
