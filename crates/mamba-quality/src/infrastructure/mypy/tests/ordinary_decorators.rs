//! Ordinary decorator diagnostics match the fixed original strict profile.
use crate::domain::{model::CompletionStatus, ports::ProjectStore, source::PythonVersion};
use crate::infrastructure::filesystem::Filesystem;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs;

#[test]
fn pinned_original_ordinary_decorators() {
    let corpus = include_str!("ordinary_decorators.json");
    assert_eq!(
        format!("{:x}", Sha256::digest(corpus.as_bytes())),
        "2fea0a802230b1001ae6f8e84175d77174703d33bb64a7fdc81bf2ccf7c1cc88"
    );
    let cases: Vec<Value> = serde_json::from_str(corpus).unwrap();
    assert_eq!(cases.len(), 8);
    let mut results = Vec::new();
    let mut failures = 0;
    for case in cases {
        let root = tempfile::tempdir().unwrap();
        let root_path = fs::canonicalize(root.path()).unwrap();
        let path = root_path.join(case["filename"].as_str().unwrap());
        let source = case["source"].as_str().unwrap();
        assert_eq!(
            format!("{:x}", Sha256::digest(source.as_bytes())),
            case["source_sha256"].as_str().unwrap()
        );
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
            assert_eq!(fs::read(&path).unwrap(), source.as_bytes());
            let diagnostic_exit = i32::from(
                report
                    .diagnostics
                    .iter()
                    .any(|diagnostic| diagnostic.severity == "error"),
            );
            let actual: Vec<_> = report.diagnostics.iter().map(|diagnostic| json!({
                "rule": diagnostic.rule, "message": diagnostic.message, "hint": diagnostic.hint,
                "path": diagnostic.path.strip_prefix(&root_path).unwrap(), "severity": diagnostic.severity,
                "location": {"line": diagnostic.location.line, "column": diagnostic.location.column,
                    "end_line": diagnostic.location.end_line, "end_column": diagnostic.location.end_column},
            })).collect();
            let actual = Value::Array(actual);
            let expected = &case["expected"][target];
            let equal = actual == *expected
                && case["reference_exits"][target].as_i64() == Some(i64::from(diagnostic_exit));
            failures += usize::from(!equal);
            results.push(json!({
                "name": case["name"], "source": source, "python": version.name(),
                "expected": expected, "actual": actual, "equal": equal,
                "reference_exit": case["reference_exits"][target],
                "diagnostic_exit": diagnostic_exit, "actual_completion": report.completion,
                "actual_reference_exit": report.reference_exit_code,
            }));
        }
    }
    assert_eq!(results.len(), 24);
    if let Some(destination) = std::env::var_os("MAMBA_QUALITY_MYPY_ORDINARY_DECORATOR_EVIDENCE") {
        fs::write(
            destination,
            serde_json::to_vec_pretty(&json!({
                "schema": "mamba.quality.mypy-ordinary-decorator-comparison.v1",
                "corpus_sha256": format!("{:x}", Sha256::digest(corpus.as_bytes())),
                "cases": results, "failed": failures, "whole_suite_accepted": false,
            }))
            .unwrap(),
        )
        .unwrap();
    }
    assert_eq!(failures, 0, "Pinned ordinary decorator comparisons failed");
}
