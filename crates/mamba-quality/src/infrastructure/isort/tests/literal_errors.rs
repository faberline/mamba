//! Exact literal directive API errors and public CLI outcomes from isort 9.0.2.
use super::super::*;
use crate::domain::{ports::ProjectStore, source::PythonVersion};
use crate::infrastructure::filesystem::Filesystem;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs;

#[test]
fn pinned_original_literal_error_messages() {
    let corpus = include_str!("literal_errors.json");
    let cases: Vec<Value> = serde_json::from_str(corpus).unwrap();
    let root = tempfile::tempdir().unwrap();
    let mut results = Vec::new();
    let mut failures = 0;
    for (index, case) in cases.into_iter().enumerate() {
        let path = root.path().join(format!("case_{index}.py"));
        let source = case["source"].as_str().unwrap();
        fs::write(&path, source).unwrap();
        assert_eq!(
            format!("{:x}", Sha256::digest(source.as_bytes())),
            case["source_sha256"].as_str().unwrap()
        );
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
            let expected = &case["expected"][target];
            let actual = match format(&workspace.sources[0], &workspace, &FixedProfile::locked()) {
                Ok((output, remaining)) => json!({
                    "output":output,"remaining":remaining,
                    "error_class":null,"message":null,
                    "reference_exit":i32::from(output != source),
                }),
                Err(error) => json!({
                    "output":null,"remaining":[],"error_class":error.code,
                    "message":error.message,
                    "reference_exit":if error.code == "ValueError" { 0 } else { 1 },
                }),
            };
            let equal = ["output", "error_class", "message", "reference_exit"]
                .iter()
                .all(|key| actual[*key] == expected[*key]);
            let transformed = transform(&workspace, &FixedProfile::locked());
            let issue_equal = expected.get("error_class").is_none()
                || transformed.issues.iter().any(|issue| {
                    issue.code == expected["error_class"].as_str().unwrap()
                        && issue.message == expected["message"].as_str().unwrap()
                });
            let equal = equal && issue_equal;
            failures += usize::from(!equal);
            results.push(json!({"name":case["name"],"python":version.name(),
                "expected":expected,"actual":actual,"equal":equal,"issue_equal":issue_equal,
                "completion":transformed.completion}));
            assert_eq!(fs::read(&path).unwrap(), source.as_bytes());
        }
    }
    if let Some(destination) = std::env::var_os("MAMBA_QUALITY_ISORT_LITERAL_ERRORS_EVIDENCE") {
        fs::write(
            destination,
            serde_json::to_vec_pretty(&json!({
                "schema":"mamba.quality.isort-literal-errors-comparison.v1",
                "corpus_sha256":format!("{:x}",Sha256::digest(corpus.as_bytes())),
                "cases":results,"failed":failures,"whole_suite_accepted":false,
            }))
            .unwrap(),
        )
        .unwrap();
    }
    assert_eq!(failures, 0, "Pinned isort literal error comparisons failed");
}
