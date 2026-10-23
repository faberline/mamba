//! Frozen Black 26.10.0 comparisons for adjacent strings around fmt directives.
use super::super::*;
use crate::domain::{
    ports::{ProjectStore, QualityEngine},
    source::PythonVersion,
};
use crate::infrastructure::filesystem::Filesystem;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs;

#[test]
fn pinned_adjacent_string_directive_comparisons() {
    compare(
        include_str!("string_directives.json"),
        60,
        "MAMBA_QUALITY_BLACK_STRING_DIRECTIVES_EVIDENCE",
        "mamba.quality.black-string-directives-comparison.v1",
    );
}

pub(super) fn compare(corpus: &str, count: usize, evidence: &str, schema: &str) {
    let cases: Vec<Value> = serde_json::from_str(corpus).unwrap();
    assert_eq!(cases.len(), count);
    let root = tempfile::tempdir().unwrap();
    let mut results = Vec::new();
    let mut failures = 0;
    let mut stable_failures = 0;
    let mut structure_failures = 0;
    let mut public_status_failures = 0;
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
                Err(issue) => (None, Some(issue.message), Some(issue.code)),
            };
            let kind = if output.is_some() {
                "formatted"
            } else {
                "error"
            };
            let reference_exit = output
                .as_ref()
                .map_or(123, |output| i32::from(output != source));
            let actual = json!({
                "kind":kind,
                "reference_exit":reference_exit,
                "exception_type":error_class,
                "message":error,
                "output":output,
            });
            let fields_match = ["kind", "reference_exit", "exception_type", "output"]
                .iter()
                .all(|key| actual[*key] == expected[*key]);
            let message_match = error_message_equal(
                actual["message"].as_str(),
                expected["message"].as_str(),
                expected["exception_type"].as_str(),
                case["invalid_output_sha256"][target].as_str(),
            );

            let public = check(&workspace);
            let expected_completion = if output.is_some() {
                CompletionStatus::Incomplete
            } else {
                CompletionStatus::Error
            };
            let public_status_matches = public.completion == expected_completion;
            let public_error_matches = if expected["kind"] == "error" {
                public.diagnostics.iter().any(|diagnostic| {
                    diagnostic.rule == expected["exception_type"].as_str().unwrap()
                        && error_message_equal(
                            Some(&diagnostic.message),
                            expected["message"].as_str(),
                            expected["exception_type"].as_str(),
                            case["invalid_output_sha256"][target].as_str(),
                        )
                })
            } else {
                true
            };
            public_status_failures += usize::from(!public_status_matches || !public_error_matches);

            let structure_equivalent = output.as_ref().map(|output| {
                // Identical bytes preserve structure even when this target cannot parse them.
                output == source
                    || crate::infrastructure::native::NativeEngine
                        .validate(&workspace.sources[0], output, &workspace, true)
                        .is_ok()
            });
            let structure_matches = structure_equivalent != Some(false);
            structure_failures += usize::from(!structure_matches);

            let (stable, repeated, repeat_error) = if let Some(output) = actual["output"].as_str() {
                workspace.sources[0].text = output.into();
                match format(&workspace.sources[0], &workspace) {
                    Ok(second) => (second == output, Some(second), None),
                    Err(issue) => (false, None, Some(issue.message)),
                }
            } else {
                (true, None, None)
            };
            stable_failures += usize::from(!stable);
            let equal = fields_match
                && message_match
                && public_status_matches
                && public_error_matches
                && structure_matches;
            failures += usize::from(!equal);
            source_preserved &= fs::read(&path).unwrap() == source.as_bytes();
            results.push(json!({
                "name":case["name"],
                "python":version.name(),
                "expected":expected,
                "actual":actual,
                "equal":equal,
                "stable":stable,
                "structure_equivalent":structure_equivalent,
                "public_completion":public.completion,
                "public_status_matches":public_status_matches,
                "public_error_matches":public_error_matches,
                "repeated":repeated,
                "repeat_error":repeat_error,
            }));
        }

        source_preservation_failures += usize::from(!source_preserved);
    }

    if let Some(destination) = std::env::var_os(evidence) {
        fs::write(
            destination,
            serde_json::to_vec_pretty(&json!({
                "schema":schema,
                "corpus_sha256":format!("{:x}", Sha256::digest(corpus.as_bytes())),
                "comparisons":results,
                "failed":failures,
                "stable_failures":stable_failures,
                "structure_failures":structure_failures,
                "public_status_failures":public_status_failures,
                "source_preservation_failures":source_preservation_failures,
                "whole_suite_accepted":false,
            }))
            .unwrap(),
        )
        .unwrap();
    }

    assert_eq!(results.len(), count * 3);
    assert_eq!(
        failures, 0,
        "Frozen Black string/directive comparisons failed"
    );
    assert_eq!(
        stable_failures, 0,
        "Black string/directive formatting did not converge"
    );
    assert_eq!(
        structure_failures, 0,
        "Black string/directive formatting changed the AST"
    );
    assert_eq!(
        public_status_failures, 0,
        "Black public check status differs from expectations"
    );
    assert_eq!(
        source_preservation_failures, 0,
        "Black string/directive formatting changed a source file"
    );
}

fn error_message_equal(
    actual: Option<&str>,
    expected: Option<&str>,
    error_class: Option<&str>,
    invalid_output_sha256: Option<&str>,
) -> bool {
    let (Some(actual), Some(expected)) = (actual, expected) else {
        return actual == expected;
    };
    if error_class == Some("black.parsing.ASTSafetyError") {
        return super::safety::message_equal(actual, expected);
    }
    let header = "INTERNAL ERROR: Black 26.10.0 on Python (CPython) ";
    if error_class != Some("black.parsing.InvalidInput") || !expected.starts_with(header) {
        return actual == expected;
    }
    if !actual.starts_with(header) {
        return false;
    }
    let marker = "This invalid output might be helpful: ";
    let (Some((actual_prefix, actual_path)), Some((expected_prefix, expected_path))) =
        (actual.rsplit_once(marker), expected.rsplit_once(marker))
    else {
        return false;
    };
    let log_name = |path: &str| {
        std::path::Path::new(path)
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with("blk_") && name.ends_with(".log"))
    };
    if actual_prefix != expected_prefix || !log_name(actual_path) || !log_name(expected_path) {
        return false;
    }
    let (Some(expected_hash), Ok(bytes)) = (invalid_output_sha256, fs::read(actual_path)) else {
        return false;
    };
    format!("{:x}", Sha256::digest(bytes)) == expected_hash
}
