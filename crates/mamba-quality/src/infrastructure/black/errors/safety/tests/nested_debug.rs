//! Compare the frozen nested debug-field AST controls with original Black bytes.
use super::super::{ast, tree};
use mamba_language::application::quality_syntax::{self, PythonTarget};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::fs;

const FIXTURE: &str = include_str!("nested_debug.json");
const PROPOSAL_SHA256: &str = "207e8606e21484146eeb8b7051906fc9c3e5271f2394ed6f4750f7485ae96dc3";
const FIXTURE_SHA256: &str = "f4ed4af5fe40acfe6d321dc1d4d32ef092d75d8b678c110c16816e6ac05f80dd";
const TARGETS: [(&str, PythonTarget); 3] = [
    ("3.12", PythonTarget::Py312),
    ("3.13", PythonTarget::Py313),
    ("3.14", PythonTarget::Py314),
];

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn bytes_record(bytes: &[u8]) -> Value {
    json!({
        "size": bytes.len(),
        "sha256": sha256(bytes),
        "bytes_hex": bytes.iter().map(|byte| format!("{byte:02x}")).collect::<String>(),
        "utf8": std::str::from_utf8(bytes).ok(),
    })
}

fn decode_hex(text: &str) -> Result<Vec<u8>, String> {
    if !text.len().is_multiple_of(2) {
        return Err("hex input has odd length".into());
    }
    text.as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let digit = |byte: u8| match byte {
                b'0'..=b'9' => Ok(byte - b'0'),
                b'a'..=b'f' => Ok(byte - b'a' + 10),
                b'A'..=b'F' => Ok(byte - b'A' + 10),
                _ => Err("hex input has a non-hex digit".to_string()),
            };
            Ok((digit(pair[0])? << 4) | digit(pair[1])?)
        })
        .collect()
}

fn syntax_issues(document: &quality_syntax::SyntaxDocument<'_>) -> Vec<Value> {
    document
        .issues
        .iter()
        .map(|issue| {
            json!({
                "message": issue.message,
                "start": issue.start,
                "end": issue.end,
                "version_error": issue.version_error,
            })
        })
        .collect()
}

fn emit_evidence(evidence: &Value) {
    if let Some(path) = std::env::var_os("MAMBA_QUALITY_BLACK_NESTED_DEBUG_EVIDENCE") {
        fs::write(
            path,
            serde_json::to_vec_pretty(evidence).expect("serialize nested debug evidence"),
        )
        .expect("write nested debug evidence");
    }
}

#[test]
fn compares_only_original_valid_nested_debug_ast_dumps() {
    let fixture: Value = serde_json::from_str(FIXTURE).expect("valid frozen nested debug fixture");
    assert_eq!(sha256(FIXTURE.as_bytes()), FIXTURE_SHA256);
    assert_eq!(
        fixture["schema"],
        "mamba.quality.black-template-nested-debug-controls.v1"
    );
    assert_eq!(
        fixture["source_provenance"]["fixture_proposal_sha256"],
        PROPOSAL_SHA256
    );
    assert_eq!(fixture["source_provenance"]["original_source_count"], 3);
    assert_eq!(fixture["source_provenance"]["reference_source_cells"], 9);
    assert_eq!(
        fixture["source_provenance"]["reference_valid_dump_cells"],
        3
    );
    assert_eq!(
        fixture["source_provenance"]["reference_syntax_error_cells_not_passes"],
        6
    );
    assert_eq!(fixture["source_provenance"]["normalization"], "none");
    assert_eq!(fixture["reference_parse_errors_are_not_comparisons"], true);

    let mut cells = Vec::new();
    let mut valid_dump_cells = 0usize;
    let mut parse_error_cells_not_compared = 0usize;
    let mut failed_valid_cells = 0usize;
    let mut native_build_errors = 0usize;
    for case in fixture["source_cases"].as_array().expect("source cases") {
        let source_bytes = decode_hex(case["source_hex"].as_str().expect("source hex"))
            .expect("frozen source hex");
        let source_sha = sha256(&source_bytes);
        let source_hash_matches = case["source_sha256"].as_str() == Some(source_sha.as_str())
            && case["source_bytes"].as_u64() == Some(source_bytes.len() as u64);
        let source = std::str::from_utf8(&source_bytes).expect("frozen Python source is UTF-8");
        for (label, target) in TARGETS {
            let expected = &case["targets"][label];
            let reference_status = expected["reference_status"]
                .as_str()
                .expect("reference status");
            let parsed = quality_syntax::parse(source, target, false);
            if reference_status == "parse_error" {
                parse_error_cells_not_compared += 1;
                let mut row = Map::new();
                row.insert("case".into(), case["id"].clone());
                row.insert("filename".into(), case["filename"].clone());
                row.insert("target".into(), json!(label));
                row.insert("source".into(), bytes_record(&source_bytes));
                row.insert("source_hash_matches".into(), json!(source_hash_matches));
                row.insert(
                    "comparison_status".into(),
                    json!("reference_parse_error_not_compared"),
                );
                row.insert(
                    "reference_error_class".into(),
                    expected["error_class"].clone(),
                );
                row.insert(
                    "reference_error_message".into(),
                    expected["error_message"].clone(),
                );
                row.insert(
                    "reference_traceback".into(),
                    bytes_record(&decode_hex(expected["traceback_hex"].as_str().unwrap()).unwrap()),
                );
                match parsed {
                    Ok(document) => {
                        row.insert(
                            "native_parse_state".into(),
                            json!(if document.is_valid() {
                                "valid"
                            } else {
                                "issues"
                            }),
                        );
                        row.insert(
                            "native_syntax_issues".into(),
                            Value::Array(syntax_issues(&document)),
                        );
                    }
                    Err(error) => {
                        row.insert("native_parse_state".into(), json!("error"));
                        row.insert("native_parse_error".into(), json!(error));
                    }
                }
                cells.push(Value::Object(row));
                if !source_hash_matches || reference_status != "parse_error" {
                    failed_valid_cells += 1;
                }
                continue;
            }

            if reference_status != "dumped" {
                failed_valid_cells += 1;
                cells.push(json!({
                    "case": case["id"], "filename": case["filename"], "target": label,
                    "source": bytes_record(&source_bytes),
                    "comparison_status": "unknown_reference_status_failure",
                    "reference_status": reference_status,
                    "source_hash_matches": source_hash_matches,
                }));
                continue;
            }
            valid_dump_cells += 1;
            let expected_dump =
                decode_hex(expected["dump_hex"].as_str().unwrap()).expect("frozen AST dump hex");
            let expected_hash_matches = sha256(&expected_dump)
                == expected["dump_sha256"].as_str().unwrap()
                && expected["dump_bytes"].as_u64() == Some(expected_dump.len() as u64);
            let (actual_dump, native_outcome, native_details) = match parsed {
                Err(error) => (None, "native_parse_error", json!({"parse_error": error})),
                Ok(document) if !document.is_valid() => {
                    let issues = syntax_issues(&document);
                    (
                        None,
                        "native_parser_rejected_reference_valid_source",
                        json!({"syntax_issues": issues}),
                    )
                }
                Ok(document) => match ast::module(&document) {
                    Err(error) => (
                        None,
                        "native_ast_build_error",
                        json!({"build_error": error}),
                    ),
                    Ok(module) => (
                        Some(tree::stringify(&module).into_bytes()),
                        "rendered",
                        json!({"syntax_issues": []}),
                    ),
                },
            };
            let dump_matches = actual_dump
                .as_ref()
                .is_some_and(|actual| actual == &expected_dump);
            let passed = source_hash_matches && expected_hash_matches && dump_matches;
            failed_valid_cells += usize::from(!passed);
            let native_build_error = native_details
                .get("build_error")
                .cloned()
                .unwrap_or(Value::Null);
            native_build_errors += usize::from(native_build_error.is_string());
            cells.push(json!({
                "case": case["id"], "filename": case["filename"], "target": label,
                "source": bytes_record(&source_bytes),
                "reference_status": reference_status,
                "expected_dump": bytes_record(&expected_dump),
                "native_outcome": native_outcome,
                "native_details": native_details,
                "native_build_error": native_build_error,
                "actual_dump": actual_dump.as_ref().map(|actual| bytes_record(actual)).unwrap_or(Value::Null),
                "source_hash_matches": source_hash_matches,
                "expected_dump_hash_matches": expected_hash_matches,
                "dump_bytes_match": dump_matches,
                "passed": passed,
            }));
        }
    }

    let evidence = json!({
        "schema": "mamba.quality.black-template-nested-debug-native-evidence.v1",
        "fixture_proposal_sha256": PROPOSAL_SHA256,
        "fixture_sha256": FIXTURE_SHA256,
        "cells": cells,
        "valid_dump_cells": valid_dump_cells,
        "reference_parse_error_cells_not_compared": parse_error_cells_not_compared,
        "failed_valid_cells": failed_valid_cells,
        "native_build_errors": native_build_errors,
        "whole_acceptance": false,
    });
    emit_evidence(&evidence);
    assert_eq!(
        valid_dump_cells, 3,
        "all original-valid AST cells must be checked"
    );
    assert_eq!(
        parse_error_cells_not_compared, 6,
        "original syntax errors are not comparisons"
    );
    assert_eq!(
        failed_valid_cells, 0,
        "frozen original AST dump comparisons failed"
    );
}
