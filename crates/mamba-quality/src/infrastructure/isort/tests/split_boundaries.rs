//! The original formatter distinguishes exact split markers from ordinary comments.
use super::*;
use crate::domain::source::PythonVersion;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

fn decode(value: &Value) -> Vec<u8> {
    let hex = value.as_str().unwrap();
    assert_eq!(hex.len() % 2, 0);
    (0..hex.len())
        .step_by(2)
        .map(|offset| u8::from_str_radix(&hex[offset..offset + 2], 16).unwrap())
        .collect()
}

#[test]
fn pinned_original_split_boundaries() {
    let corpus = include_str!("split_boundaries.json");
    assert_eq!(
        format!("{:x}", Sha256::digest(corpus.as_bytes())),
        "a2a6826efc10662afe6cbea32f906e403207cfebde2d604e5651a6c33b37844b"
    );
    let cases: Vec<Value> = serde_json::from_str(corpus).unwrap();
    assert_eq!(cases.len(), 18);
    let mut results = Vec::new();
    let mut failures = 0;
    for case in cases {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("main.py");
        let source = decode(&case["source_hex"]);
        assert_eq!(
            format!("{:x}", Sha256::digest(&source)),
            case["source_sha256"].as_str().unwrap()
        );
        for (target, version) in [
            PythonVersion::Py312,
            PythonVersion::Py313,
            PythonVersion::Py314,
        ]
        .into_iter()
        .enumerate()
        {
            fs::write(&path, &source).unwrap();
            let mut workspace = Filesystem.load(std::slice::from_ref(&path), true);
            workspace.projects[0].python_version = version;
            let (output, remaining) =
                format(&workspace.sources[0], &workspace, &FixedProfile::locked()).unwrap();
            assert!(remaining.is_empty());
            let bytes = Filesystem
                .candidate_bytes(&workspace.sources[0], &output)
                .unwrap();
            let expected = &case["expected"][target];
            assert_eq!(expected["python"], version.name());
            let original_bytes = decode(&expected["output_hex"]);
            assert_eq!(
                format!("{:x}", Sha256::digest(&original_bytes)),
                expected["output_sha256"].as_str().unwrap()
            );
            let changed = i32::from(bytes != source);
            let equal = bytes == original_bytes
                && expected["check_exit"].as_i64() == Some(i64::from(changed));
            failures += usize::from(!equal);
            fs::write(&path, &bytes).unwrap();
            let mut repeated_workspace = Filesystem.load(std::slice::from_ref(&path), true);
            repeated_workspace.projects[0].python_version = version;
            let (repeated, remaining) = format(
                &repeated_workspace.sources[0],
                &repeated_workspace,
                &FixedProfile::locked(),
            )
            .unwrap();
            let repeated_bytes = Filesystem
                .candidate_bytes(&repeated_workspace.sources[0], &repeated)
                .unwrap();
            let stable = remaining.is_empty() && repeated_bytes == bytes;
            failures += usize::from(!stable);
            results.push(json!({"name":case["name"],"python":version.name(),"equal":equal,"stable":stable,
                "expected":String::from_utf8(original_bytes).unwrap(),"actual":String::from_utf8(bytes).unwrap(),
                "reference_check_exit":expected["check_exit"],"candidate_changed_exit":changed}));
        }
    }
    assert_eq!(results.len(), 54);
    if let Some(destination) = std::env::var_os("MAMBA_QUALITY_ISORT_SPLIT_BOUNDARY_EVIDENCE") {
        fs::write(
            destination,
            serde_json::to_vec_pretty(&json!({
                "schema":"mamba.quality.isort-split-boundary-comparison.v1",
                "cases":results,"failed":failures,"whole_suite_accepted":false,
            }))
            .unwrap(),
        )
        .unwrap();
    }
    assert_eq!(
        failures, 0,
        "Pinned isort split boundary comparisons failed"
    );
}
