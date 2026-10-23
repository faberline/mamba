//! Multi-file originals retain every diagnostic and the original file order.
mod catalog;
use crate::domain::{model::CompletionStatus, ports::ProjectStore, source::PythonVersion};
use crate::infrastructure::filesystem::Filesystem;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs;

#[test]
fn pinned_original_projects() {
    let cases: Vec<Value> = catalog::CHUNKS
        .iter()
        .flat_map(|chunk| serde_json::from_str::<Vec<Value>>(chunk).unwrap())
        .collect();
    assert_eq!(cases.len(), catalog::CASE_COUNT);
    let mut results = Vec::new();
    let mut failures = 0;
    for case in cases {
        let root = tempfile::tempdir().unwrap();
        let root_path = fs::canonicalize(root.path()).unwrap();
        fs::write(
            root_path.join("pyproject.toml"),
            "[project]\nname = \"fixture\"\nversion = \"0.0.0\"\nrequires-python = \">=3.12\"\n",
        )
        .unwrap();
        let files = case["files"].as_object().unwrap();
        for (filename, contents) in files {
            let source = contents.as_str().unwrap();
            assert_eq!(
                format!("{:x}", Sha256::digest(source.as_bytes())),
                case["source_sha256"][filename].as_str().unwrap()
            );
            let path = root_path.join(filename);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, source).unwrap();
        }
        let mut workspace = Filesystem.load(std::slice::from_ref(&root_path), true);
        assert_eq!(workspace.sources.len(), files.len());
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
            assert_eq!(report.checked_files, files.len());
            let actual: Vec<_> = report.diagnostics.iter().map(|diagnostic| json!({
                "rule": serde_json::to_value(diagnostic).unwrap()["rule"], "message": diagnostic.message, "hint": diagnostic.hint,
                "path": diagnostic.path.strip_prefix(&root_path).unwrap(), "severity": diagnostic.severity,
                "location": {"line": diagnostic.location.line, "column": diagnostic.location.column,
                    "end_line": diagnostic.location.end_line, "end_column": diagnostic.location.end_column},
            })).collect();
            let actual = Value::Array(actual);
            let expected = &case["expected"][target];
            let equal = actual == *expected;
            failures += usize::from(!equal);
            results.push(
                json!({"name": case["name"], "files": files, "python": version.name(),
                "expected": expected, "actual": actual, "equal": equal,
                "reference_exit": case["reference_exits"][target]}),
            );
        }
    }
    if let Some(destination) = std::env::var_os("MAMBA_QUALITY_MYPY_PROJECT_EVIDENCE") {
        fs::write(destination, serde_json::to_vec_pretty(&json!({
            "schema": "mamba.quality.mypy-project-comparison.v1", "corpus_sha256": catalog::CORPUS_SHA256,
            "cases": results, "failed": failures, "whole_suite_accepted": false,
        })).unwrap()).unwrap();
    }
    assert_eq!(
        failures, 0,
        "Pinned multi-file mypy diagnostic comparisons failed"
    );
}
