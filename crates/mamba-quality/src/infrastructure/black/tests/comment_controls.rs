//! Frozen Black results for trailing comment control characters.
use super::super::*;
use crate::domain::{
    ports::{ProjectStore, QualityEngine},
    source::PythonVersion,
};
use crate::infrastructure::{filesystem::Filesystem, native::NativeEngine};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs;

const CORPUS_SHA256: &str = "7a694c79aa7483b91c7598e522b7d2beeba58373680be59c35857702fa8290cf";

#[test]
fn pinned_comment_controls() {
    let corpus = include_str!("comment_controls.json");
    let cases: Vec<Value> = serde_json::from_str(corpus).unwrap();
    assert_eq!(cases.len(), 13);
    assert_eq!(
        format!("{:x}", Sha256::digest(corpus.as_bytes())),
        CORPUS_SHA256
    );
    let root = tempfile::tempdir().unwrap();
    let root_path = fs::canonicalize(root.path()).unwrap();
    let mut comparisons = Vec::new();
    let mut failures = 0;
    let mut public_failures = 0;
    let mut structure_failures = 0;
    let mut stability_failures = 0;
    let mut source_preservation_failures = 0;

    for (index, case) in cases.into_iter().enumerate() {
        let path = root_path.join(format!("case_{index}.py"));
        let source = case["source"].as_str().unwrap();
        fs::write(&path, source).unwrap();
        assert_eq!(
            format!("{:x}", Sha256::digest(source.as_bytes())),
            case["source_sha256"]
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
            let fields_match = kind == expected["kind"]
                && output.as_deref() == expected["output"].as_str()
                && reference_exit == expected["reference_exit"]
                && error_class.as_deref() == expected["exception_type"].as_str()
                && error.as_deref() == expected["message"].as_str();

            let public = check(&workspace);
            let expected_completion = if expected["kind"] == "error" {
                CompletionStatus::Error
            } else {
                CompletionStatus::Incomplete
            };
            let public_status_matches = public.completion == expected_completion;
            let public_message_matches = if expected["kind"] == "error" {
                public.diagnostics.iter().any(|diagnostic| {
                    Some(diagnostic.rule.as_str()) == expected["exception_type"].as_str()
                        && Some(diagnostic.message.as_str()) == expected["message"].as_str()
                })
            } else if expected["reference_exit"] == 1 {
                public.diagnostics.len() == 1 && {
                    let diagnostic = &public.diagnostics[0];
                    diagnostic.tool == QualityTool::Black
                        && diagnostic.rule == "format"
                        && diagnostic.message == format!("would reformat {}", path.display())
                        && diagnostic.hint.is_none()
                        && diagnostic.path == path
                        && diagnostic.severity == "error"
                        && diagnostic.location.line == 1
                        && diagnostic.location.column == 1
                        && diagnostic.location.end_line.is_none()
                        && diagnostic.location.end_column.is_none()
                        && diagnostic.location.offset == Some(0)
                        && diagnostic.location.end_offset.is_none()
                        && diagnostic.fix.is_none()
                }
            } else {
                public.diagnostics.is_empty()
            };
            assert_eq!(public.reference_exit_code, None);
            assert_eq!(public.checked_files, 1);
            public_failures += usize::from(!public_status_matches || !public_message_matches);

            let structure_equivalent = output.as_ref().map(|output| {
                output == source
                    || NativeEngine
                        .validate(&workspace.sources[0], output, &workspace, true)
                        .is_ok()
            });
            let structure_matches = structure_equivalent != Some(false);
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
            let equal = fields_match
                && public_status_matches
                && public_message_matches
                && structure_matches
                && stable;
            failures += usize::from(!equal);
            let source_preserved = fs::read(&path).unwrap() == source.as_bytes();
            source_preservation_failures += usize::from(!source_preserved);
            comparisons.push(json!({
                "name":case["name"],
                "python":version.name(),
                "expected":expected,
                "actual":{"kind":kind,"reference_exit":reference_exit,"exception_type":error_class,"message":error,"output":output},
                "equal":equal,
                "stable":stable,
                "repeated":repeated,
                "repeat_error":repeat_error,
                "structure_equivalent":structure_equivalent,
                "public_completion":public.completion,
                "public_status_matches":public_status_matches,
                "public_message_matches":public_message_matches,
                "source_preserved":source_preserved,
            }));
        }
    }

    if let Some(destination) = std::env::var_os("MAMBA_QUALITY_BLACK_COMMENT_CONTROLS_EVIDENCE") {
        fs::write(
            destination,
            serde_json::to_vec_pretty(&json!({
                "schema":"mamba.quality.black-comment-controls-comparison.v1",
                "corpus_sha256":format!("{:x}", Sha256::digest(corpus.as_bytes())),
                "comparisons":comparisons,
                "failed":failures,
                "public_failures":public_failures,
                "structure_failures":structure_failures,
                "stability_failures":stability_failures,
                "source_preservation_failures":source_preservation_failures,
                "whole_suite_accepted":false
            }))
            .unwrap(),
        )
        .unwrap();
    }

    assert_eq!(comparisons.len(), 39);
    assert_eq!(failures, 0, "Frozen comment-control results differ");
    assert_eq!(
        public_failures, 0,
        "Public completion or diagnostics differ"
    );
    assert_eq!(structure_failures, 0, "Formatted structure changed");
    assert_eq!(stability_failures, 0, "Formatted result did not converge");
    assert_eq!(source_preservation_failures, 0, "A source file changed");
}
