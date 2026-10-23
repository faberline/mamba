//! Frozen Black results for mixed byte, string, and interpolated literal order.
use super::super::*;
use crate::domain::{ports::ProjectStore, source::PythonVersion};
use crate::infrastructure::filesystem::Filesystem;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs;

#[test]
fn pinned_mixed_literal_order_comparisons() {
    let corpus = include_str!("mixed_literal_order.json");
    let cases: Vec<Value> = serde_json::from_str(corpus).unwrap();
    assert_eq!(cases.len(), 8);
    assert_eq!(
        format!("{:x}", Sha256::digest(corpus.as_bytes())),
        "45b1e763c85caa93832632e2edf2ca19a79f9e2f03b05758963643fe3f5d822e"
    );
    let root = tempfile::tempdir().unwrap();
    let mut comparisons = Vec::new();
    let mut failures = 0;
    let mut public_failures = 0;
    let mut stability_failures = 0;
    let mut source_preservation_failures = 0;

    for (index, case) in cases.into_iter().enumerate() {
        let path = root.path().join(format!("case_{index}.py"));
        let source = case["source"].as_str().unwrap();
        fs::write(&path, source).unwrap();
        assert_eq!(
            format!("{:x}", Sha256::digest(source.as_bytes())),
            case["source_sha256"].as_str().unwrap(),
            "{}",
            case["name"]
        );
        let mut workspace = Filesystem.load(std::slice::from_ref(&path), true);
        let mut source_preserved = true;

        for (target, version) in [
            PythonVersion::Py312,
            PythonVersion::Py313,
            PythonVersion::Py314,
        ]
        .into_iter()
        .enumerate()
        {
            workspace.projects[0].python_version = version;
            workspace.sources[0].text = source.into();
            let expected = &case["expected"][target];
            let (output, error, error_class) = match format(&workspace.sources[0], &workspace) {
                Ok(output) => (Some(output), None, None),
                Err(issue) => (None, Some(issue.message.clone()), Some(issue.code.clone())),
            };
            let kind = if output.is_some() {
                "formatted"
            } else {
                "error"
            };
            let reference_exit = output
                .as_ref()
                .map_or(123, |output| i32::from(output != source));
            let message_matches = error.as_deref() == expected["message"].as_str();
            let status_matches = match expected["kind"].as_str() {
                Some("error") => error_class.as_deref() == expected["exception_type"].as_str(),
                Some("formatted") => error.is_none(),
                _ => false,
            };
            let public = check(&workspace);
            let public_status_matches =
                (public.completion == CompletionStatus::Error) == (expected["kind"] == "error");
            let expected_exception = expected["exception_type"].as_str();
            let public_message_matches = if let Some(exception) = expected_exception {
                public
                    .diagnostics
                    .iter()
                    .find(|diagnostic| diagnostic.rule == exception)
                    .is_some_and(|diagnostic| {
                        Some(diagnostic.message.as_str()) == expected["message"].as_str()
                    })
            } else {
                public.diagnostics.is_empty()
            };
            let public_matches = public_status_matches && public_message_matches;
            public_failures += usize::from(!public_matches);
            let (stable, repeated, repeat_error) = match format(&workspace.sources[0], &workspace) {
                Ok(second) => (
                    output.as_deref() == Some(second.as_str()),
                    Some(second),
                    None,
                ),
                Err(issue) => (
                    error.as_deref() == Some(issue.message.as_str())
                        && error_class.as_deref() == Some(issue.code.as_str()),
                    None,
                    Some(issue.message),
                ),
            };
            stability_failures += usize::from(!stable);
            let comparison_matches = kind == expected["kind"]
                && reference_exit == expected["reference_exit"]
                && error_class.as_deref() == expected_exception
                && output.as_deref() == expected["output"].as_str()
                && message_matches
                && status_matches
                && public_matches
                && stable;
            failures += usize::from(!comparison_matches);
            source_preserved &= fs::read(&path).unwrap() == source.as_bytes();
            comparisons.push(json!({
                "name":case["name"],
                "python":version.name(),
                "expected":expected,
                "actual":{"kind":kind,"reference_exit":reference_exit,"exception_type":error_class,"message":error,"output":output},
                "equal":comparison_matches,
                "stable":stable,
                "repeated":repeated,
                "repeat_error":repeat_error,
                "public_completion":public.completion,
                "public_status_matches":public_status_matches,
                "public_message_matches":public_message_matches,
            }));
        }
        source_preservation_failures += usize::from(!source_preserved);
    }

    if let Some(destination) = std::env::var_os("MAMBA_QUALITY_BLACK_MIXED_LITERAL_ORDER_EVIDENCE")
    {
        fs::write(
            destination,
            serde_json::to_vec_pretty(&json!({
                "schema":"mamba.quality.black-mixed-literal-order-comparison.v1",
                "corpus_sha256":format!("{:x}", Sha256::digest(corpus.as_bytes())),
                "comparisons":comparisons,
                "failed":failures,
                "public_failures":public_failures,
                "stability_failures":stability_failures,
                "source_preservation_failures":source_preservation_failures,
                "whole_suite_accepted":false,
            }))
            .unwrap(),
        )
        .unwrap();
    }

    assert_eq!(comparisons.len(), 24);
    assert_eq!(failures, 0, "Frozen mixed literal-order results differ");
    assert_eq!(public_failures, 0, "Public check results differ");
    assert_eq!(stability_failures, 0, "Repeated results differ");
    assert_eq!(source_preservation_failures, 0, "A source file changed");
}
