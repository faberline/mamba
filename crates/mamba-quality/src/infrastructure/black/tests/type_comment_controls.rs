//! Frozen original Black outcomes for C0 type-comment whitespace controls.
use super::super::*;
use crate::domain::{
    ports::{ProjectStore, QualityEngine},
    source::PythonVersion,
};
use crate::infrastructure::{filesystem::Filesystem, native::NativeEngine};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{fs, path::Path};

const FIXTURE_SHA256: &str = "e10f3067b0321eafc2588892b0db898635d1bbc479f639ff2f219b10897d8214";
const ORIGINAL_LOG_SHA256: &str =
    "2d6c9acd5921a70539c713c2e2f3de2fdf6b1351cfdf4cf8c512416d46c10ace";

fn bytes_from_hex(value: &str) -> Vec<u8> {
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let digit = |byte: u8| match byte {
                b'0'..=b'9' => byte - b'0',
                b'a'..=b'f' => byte - b'a' + 10,
                b'A'..=b'F' => byte - b'A' + 10,
                _ => panic!("invalid hex fixture byte"),
            };
            (digit(pair[0]) << 4) | digit(pair[1])
        })
        .collect()
}

fn bytes_to_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut result = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        result.push(HEX[(byte >> 4) as usize] as char);
        result.push(HEX[(byte & 0x0f) as usize] as char);
    }
    result
}

fn log_path(message: &str) -> Option<&Path> {
    let (_, path) = message.rsplit_once("This diff might be helpful: ")?;
    let path = Path::new(path);
    let name = path.file_name()?.to_str()?;
    let suffix = name.strip_prefix("blk_")?.strip_suffix(".log")?;
    if suffix.is_empty()
        || !suffix
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    {
        return None;
    }
    Some(path)
}

fn approved_message_equal(actual: &str, expected: &str) -> bool {
    // The existing tracker permits environment path prefixes and generated basenames.
    // Bound the native parent to this process's real temporary directory.
    super::safety::message_equal(actual, expected)
        && log_path(actual)
            .is_some_and(|path| path.parent() == Some(std::env::temp_dir().as_path()))
}

fn diagnostic_value(diagnostic: &QualityDiagnostic) -> Value {
    json!({
        "tool":diagnostic.tool,
        "rule":diagnostic.rule,
        "message":diagnostic.message,
        "hint":diagnostic.hint,
        "path":diagnostic.path,
        "severity":diagnostic.severity,
        "location":{
            "line":diagnostic.location.line,
            "column":diagnostic.location.column,
            "end_line":diagnostic.location.end_line,
            "end_column":diagnostic.location.end_column,
            "offset":diagnostic.location.offset,
            "end_offset":diagnostic.location.end_offset
        },
        "fix":diagnostic.fix
    })
}

#[test]
fn pinned_c0_type_comment_controls() {
    let corpus = include_str!("type_comment_controls.json");
    let frozen: Value = serde_json::from_str(corpus).unwrap();
    assert_eq!(
        format!("{:x}", Sha256::digest(corpus.as_bytes())),
        FIXTURE_SHA256
    );
    assert_eq!(
        frozen["source_controls_sha256"],
        "7a3c1ff42b43d682745459ffb500cdc281a309245b90ae3175891dc77bb5113e"
    );
    assert_eq!(
        frozen["worker_manifest_sha256"],
        "8aea80d6846193fc64c20338896723ef96b84d97b06162026d647c6f429d75f0"
    );

    let root = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(root.path()).unwrap();
    let versions = [
        ("3.12", PythonVersion::Py312),
        ("3.13", PythonVersion::Py313),
        ("3.14", PythonVersion::Py314),
    ];
    let mut comparisons = Vec::new();
    let mut failures = 0;
    let mut formatter_failures = 0;
    let mut public_failures = 0;
    let mut structure_failures = 0;
    let mut stability_failures = 0;
    let mut source_preservation_failures = 0;
    let controls = frozen["controls"].as_array().unwrap();

    for (case_index, case) in controls.iter().enumerate() {
        let source_bytes = bytes_from_hex(case["source_hex"].as_str().unwrap());
        let source = String::from_utf8(source_bytes.clone()).unwrap();
        let path = root.join(format!("case_{case_index}.py"));
        fs::write(&path, &source_bytes).unwrap();
        let source_sha256 = format!("{:x}", Sha256::digest(&source_bytes));
        let source_hash_matches = source_sha256 == case["source_sha256"].as_str().unwrap();
        let mut workspace = Filesystem.load(std::slice::from_ref(&path), true);
        let loaded = workspace
            .sources
            .first()
            .is_some_and(|loaded| loaded.text == source);

        for (target_index, (target_name, version)) in versions.iter().enumerate() {
            workspace.projects[0].python_version = *version;
            workspace.sources[0].text = source.clone();
            let expected = &case["expected"][target_index]["expected"];
            let target_matches = case["expected"][target_index]["target"] == *target_name;
            let expected_kind = expected["kind"].as_str().unwrap();
            let format_result = format(&workspace.sources[0], &workspace);
            let (actual_kind, output, error_code, error_message, actual_log) = match format_result {
                Ok(output) => ("formatted", Some(output), None, None, None),
                Err(issue) => {
                    let log = log_path(&issue.message).and_then(|log_path| fs::read(log_path).ok());
                    ("error", None, Some(issue.code), Some(issue.message), log)
                }
            };
            let actual_output_bytes = output.as_ref().map(String::as_bytes);
            let computed_reference_exit = if error_code.is_some() {
                123
            } else {
                i32::from(actual_output_bytes != Some(source.as_bytes()))
            };
            let expected_error_code = expected["exception_type"].as_str();
            let expected_error_message = expected["message"].as_str();
            let message_matches = match (&error_message, expected_error_message) {
                (Some(actual), Some(expected)) if expected_kind == "error" => {
                    approved_message_equal(actual, expected)
                }
                (None, None) => true,
                _ => false,
            };
            let actual_log_sha256 = actual_log
                .as_ref()
                .map(|bytes| format!("{:x}", Sha256::digest(bytes)));
            let actual_log_hex = actual_log.as_deref().map(bytes_to_hex);
            let log_matches = if expected_kind == "error" {
                expected_error_code == Some("black.parsing.ASTSafetyError")
                    && actual_log_sha256.as_deref() == Some(ORIGINAL_LOG_SHA256)
                    && actual_log_sha256.as_deref() == expected["error_log_sha256"].as_str()
                    && actual_log_hex.as_deref() == expected["error_log_hex"].as_str()
            } else {
                actual_log.is_none()
            };
            let output_matches = actual_kind == expected_kind
                && actual_output_bytes.map(bytes_to_hex).as_deref()
                    == expected["output_hex"].as_str()
                && computed_reference_exit == expected["reference_exit"].as_i64().unwrap() as i32
                && error_code.as_deref() == expected_error_code
                && message_matches
                && log_matches;

            let structure_equivalent = output.as_ref().map(|output| {
                output == &source
                    || NativeEngine
                        .validate(&workspace.sources[0], output, &workspace, true)
                        .is_ok()
            });
            let structure_matches = structure_equivalent != Some(false);
            let public = check(&workspace);
            let expected_completion = if expected_kind == "error" {
                CompletionStatus::Error
            } else {
                CompletionStatus::Incomplete
            };
            let expected_checked_files = if expected_kind == "error" { 0 } else { 1 };
            let public_diagnostics: Vec<Value> =
                public.diagnostics.iter().map(diagnostic_value).collect();
            let public_diagnostics_match = if expected_kind == "error" {
                public.diagnostics.len() == 1
                    && public.diagnostics[0].rule == "black.parsing.ASTSafetyError"
                    && approved_message_equal(
                        &public.diagnostics[0].message,
                        expected_error_message.unwrap(),
                    )
                    && public.diagnostics[0].severity == "error"
                    && public.diagnostics[0].path == path
                    && public.diagnostics[0].hint.is_none()
                    && public.diagnostics[0].location.line == 1
                    && public.diagnostics[0].location.column == 1
                    && public.diagnostics[0].location.end_line.is_none()
                    && public.diagnostics[0].location.end_column.is_none()
                    && public.diagnostics[0].location.offset.is_none()
                    && public.diagnostics[0].location.end_offset.is_none()
                    && public.diagnostics[0].fix.is_none()
            } else if computed_reference_exit == 1 {
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
            let public_matches = public.completion == expected_completion
                && public.reference_exit_code.is_none()
                && public.checked_files == expected_checked_files
                && public_diagnostics_match;

            let (repeat_stable, repeated_output, repeat_error) =
                if let Some(output) = output.as_ref() {
                    workspace.sources[0].text = output.clone();
                    match format(&workspace.sources[0], &workspace) {
                        Ok(second) => (second == *output, Some(second), None),
                        Err(issue) => (
                            false,
                            None,
                            Some(json!({"code":issue.code,"message":issue.message})),
                        ),
                    }
                } else {
                    workspace.sources[0].text = source.clone();
                    match format(&workspace.sources[0], &workspace) {
                        Err(issue) => {
                            let repeat_log = log_path(&issue.message)
                                .and_then(|log_path| fs::read(log_path).ok());
                            let repeat_log_sha256 = repeat_log
                                .as_ref()
                                .map(|bytes| format!("{:x}", Sha256::digest(bytes)));
                            let repeat_log_hex = repeat_log.as_deref().map(bytes_to_hex);
                            let stable = Some(issue.code.as_str()) == expected_error_code
                                && approved_message_equal(
                                    &issue.message,
                                    expected_error_message.unwrap(),
                                )
                                && repeat_log_sha256.as_deref()
                                    == expected["error_log_sha256"].as_str()
                                && repeat_log_hex.as_deref() == expected["error_log_hex"].as_str();
                            (
                                stable,
                                None,
                                Some(json!({
                                    "code":issue.code,
                                    "message":issue.message,
                                    "log_hex":repeat_log_hex,
                                    "log_sha256":repeat_log_sha256
                                })),
                            )
                        }
                        Ok(output) => (false, Some(output), None),
                    }
                };
            let source_after = fs::read(&path).unwrap();
            let source_preserved = source_after == source_bytes;
            let source_matches = source_hash_matches && loaded;
            let equal = target_matches
                && output_matches
                && public_matches
                && structure_matches
                && repeat_stable
                && source_preserved
                && source_matches;
            failures += usize::from(!equal);
            formatter_failures += usize::from(!output_matches);
            public_failures += usize::from(!public_matches);
            structure_failures += usize::from(!structure_matches);
            stability_failures += usize::from(!repeat_stable);
            source_preservation_failures += usize::from(!source_preserved || !source_matches);

            comparisons.push(json!({
                "name":case["id"],
                "python":target_name,
                "target_matches_fixture":target_matches,
                "expected":expected,
                "actual":{
                    "source_hex":bytes_to_hex(&source_bytes),
                    "source_sha256":source_sha256,
                    "kind":actual_kind,
                    "output_hex":actual_output_bytes.map(bytes_to_hex),
                    "computed_reference_exit":computed_reference_exit,
                    "exception_type":error_code,
                    "raw_error_message":error_message,
                    "error_log_hex":actual_log_hex,
                    "error_log_sha256":actual_log_sha256,
                    "api_completion":format!("{:?}",public.completion),
                    "api_reference_exit_code":public.reference_exit_code,
                    "api_checked_files":public.checked_files,
                    "api_diagnostics":public_diagnostics
                },
                "equal":equal,
                "message_matches_under_approved_ast_safety_rule":message_matches,
                "public_matches":public_matches,
                "structure_equivalent":structure_equivalent,
                "repeat_stable":repeat_stable,
                "repeated_output_hex":repeated_output.as_deref().map(str::as_bytes).map(bytes_to_hex),
                "repeat_error":repeat_error,
                "source_preserved":source_preserved,
                "source_matches_fixture":source_matches
            }));
        }
    }

    if let Some(destination) = std::env::var_os("MAMBA_QUALITY_BLACK_C0_TYPE_COMMENT_EVIDENCE") {
        fs::write(
            destination,
            serde_json::to_vec_pretty(&json!({
                "schema":"mamba.quality.black-c0-type-comment-comparison.v1",
                "fixture_sha256":format!("{:x}",Sha256::digest(corpus.as_bytes())),
                "comparisons":comparisons,
                "cells":comparisons.len(),
                "failed":failures,
                "formatter_failures":formatter_failures,
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

    assert_eq!(comparisons.len(), 18);
    assert_eq!(failures, 0, "Frozen Black C0 type-comment outcomes differ");
    assert_eq!(
        formatter_failures, 0,
        "Black formatter result differs from original"
    );
    assert_eq!(
        public_failures, 0,
        "Black public report differs from its result contract"
    );
    assert_eq!(structure_failures, 0, "Formatted syntax changed");
    assert_eq!(stability_failures, 0, "Black result did not remain stable");
    assert_eq!(source_preservation_failures, 0, "A source input changed");
}
