//! Frozen Black 26.10.0 results for skipped multiline interpolations in adjacent strings.
use super::super::*;
use crate::domain::{
    ports::{ProjectStore, QualityEngine},
    source::PythonVersion,
};
use crate::infrastructure::filesystem::Filesystem;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{fs, path::Path};

const INVALID_MARKER: &str = "This invalid output might be helpful: ";
const INVALID_INPUT: &str = "black.parsing.InvalidInput";
const CORPUS_SHA256: &str = "85a40b7317fbd21c315643b2301434cbae261ff9a9188605d763ec41add7d735";

#[test]
fn pinned_interpolation_continuation_comparisons() {
    let corpus = include_str!("interpolation_continuation.json");
    let cases: Vec<Value> = serde_json::from_str(corpus).unwrap();
    assert_eq!(cases.len(), 8);
    assert_eq!(
        format!("{:x}", Sha256::digest(corpus.as_bytes())),
        CORPUS_SHA256
    );
    let root = tempfile::tempdir().unwrap();
    let mut comparisons = Vec::new();
    let mut failures = 0;
    let mut structure_failures = 0;
    let mut status_failures = 0;
    let mut stability_failures = 0;
    let mut source_preservation_failures = 0;
    let mut invalid_hash_failures = 0;

    for (index, case) in cases.into_iter().enumerate() {
        let source = case["source"].as_str().unwrap();
        let path = root.path().join(format!("case_{index}.py"));
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
            let (output, error, error_class, invalid_sha) =
                match format(&workspace.sources[0], &workspace) {
                    Ok(output) => (Some(output), None, None, None),
                    Err(issue) => {
                        let message = issue.message.clone();
                        let sha = saved_invalid_output_sha256(&message);
                        (None, Some(message), Some(issue.code), sha)
                    }
                };
            let kind = if output.is_some() {
                "formatted"
            } else {
                "error"
            };
            let reference_exit = output
                .as_ref()
                .map_or(123, |output| i32::from(output != source));
            let expected_message = expected["message"].as_str();
            let expected_class = expected["exception_type"].as_str();
            let message_matches = match (
                error.as_deref(),
                expected_message,
                error_class.as_deref(),
                expected_class,
            ) {
                (Some(actual), Some(expected), Some(INVALID_INPUT), Some(INVALID_INPUT)) => {
                    invalid_message_matches(actual, expected)
                }
                (actual, expected, _, _) => actual == expected,
            };
            let expected_invalid_sha = case["invalid_output_sha256"][target].as_str();
            let invalid_hash_matches = invalid_sha.as_deref() == expected_invalid_sha;
            invalid_hash_failures += usize::from(!invalid_hash_matches);

            let public = check(&workspace);
            let expected_completion = if expected["kind"] == "error" {
                CompletionStatus::Error
            } else {
                CompletionStatus::Incomplete
            };
            let completion_matches = public.completion == expected_completion;
            let public_message_matches = if expected["kind"] == "error" {
                public.diagnostics.iter().any(|diagnostic| {
                    diagnostic.rule == expected_class.unwrap()
                        && match (diagnostic.message.as_str(), expected_message) {
                            (actual, Some(expected)) if expected_class == Some(INVALID_INPUT) => {
                                invalid_message_matches(actual, expected)
                                    && saved_invalid_output_sha256(actual).as_deref()
                                        == expected_invalid_sha
                            }
                            (actual, Some(expected)) => actual == expected,
                            _ => false,
                        }
                })
            } else {
                true
            };
            status_failures += usize::from(!completion_matches || !public_message_matches);

            let structure = output.as_ref().map(|output| {
                output == source
                    || crate::infrastructure::native::NativeEngine
                        .validate(&workspace.sources[0], output, &workspace, true)
                        .is_ok()
            });
            let structure_matches = structure != Some(false);
            structure_failures += usize::from(!structure_matches);

            let (stable, repeated, repeat_error) = if let Some(output) = output.as_ref() {
                workspace.sources[0].text = output.clone();
                match format(&workspace.sources[0], &workspace) {
                    Ok(second) => (second == *output, Some(second), None),
                    Err(issue) => (false, None, Some(issue.message)),
                }
            } else {
                (true, None, None)
            };
            stability_failures += usize::from(!stable);

            let matches = kind == expected["kind"]
                && reference_exit == expected["reference_exit"]
                && error_class.as_deref() == expected_class
                && output.as_deref() == expected["output"].as_str()
                && message_matches
                && invalid_hash_matches
                && completion_matches
                && public_message_matches
                && structure_matches
                && stable;
            failures += usize::from(!matches);
            source_preserved &= fs::read(&path).unwrap() == source.as_bytes();
            comparisons.push(json!({
                "name":case["name"],
                "python":version.name(),
                "expected":expected,
                "actual":{
                    "kind":kind,
                    "reference_exit":reference_exit,
                    "exception_type":error_class,
                    "message":error,
                    "output":output,
                    "invalid_output_sha256":invalid_sha,
                },
                "equal":matches,
                "stable":stable,
                "repeated":repeated,
                "repeat_error":repeat_error,
                "structure_equivalent":structure,
                "public_completion":public.completion,
                "public_message_matches":public_message_matches,
                "invalid_output_hash_matches":invalid_hash_matches,
            }));
        }
        source_preservation_failures += usize::from(!source_preserved);
    }

    if let Some(destination) =
        std::env::var_os("MAMBA_QUALITY_BLACK_INTERPOLATION_CONTINUATION_EVIDENCE")
    {
        fs::write(
            destination,
            serde_json::to_vec_pretty(&json!({
                "schema":"mamba.quality.black-interpolation-continuation-comparison.v1",
                "corpus_sha256":format!("{:x}", Sha256::digest(corpus.as_bytes())),
                "comparisons":comparisons,
                "failed":failures,
                "structure_failures":structure_failures,
                "status_failures":status_failures,
                "stability_failures":stability_failures,
                "source_preservation_failures":source_preservation_failures,
                "invalid_hash_failures":invalid_hash_failures,
                "whole_suite_accepted":false
            }))
            .unwrap(),
        )
        .unwrap();
    }

    assert_eq!(comparisons.len(), 24);
    assert_eq!(
        failures, 0,
        "Frozen interpolation continuation results differ"
    );
    assert_eq!(structure_failures, 0, "Formatted structure changed");
    assert_eq!(
        status_failures, 0,
        "Public completion or diagnostics differ"
    );
    assert_eq!(stability_failures, 0, "Formatted result did not converge");
    assert_eq!(source_preservation_failures, 0, "A source file changed");
    assert_eq!(invalid_hash_failures, 0, "Malformed output bytes differ");
}

fn saved_invalid_output_sha256(message: &str) -> Option<String> {
    let (_, path) = message.rsplit_once(INVALID_MARKER)?;
    let path = Path::new(path);
    let name = path.file_name()?.to_str()?;
    if !name.starts_with("blk_") || !name.ends_with(".log") {
        return None;
    }
    Some(format!("{:x}", Sha256::digest(fs::read(path).ok()?)))
}

fn invalid_message_matches(actual: &str, expected: &str) -> bool {
    let Some((actual_body, actual_path)) = actual.rsplit_once(INVALID_MARKER) else {
        return false;
    };
    let Some((expected_body, expected_path)) = expected.rsplit_once(INVALID_MARKER) else {
        return false;
    };
    if actual_body != expected_body {
        return false;
    }
    let actual_path = Path::new(actual_path);
    let expected_path = Path::new(expected_path);
    actual_path.parent() == expected_path.parent()
        && actual_path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with("blk_") && name.ends_with(".log"))
}
