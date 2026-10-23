//! Compare the private Black AST tree and unified diff renderers with frozen original bytes.
use super::{ast, diff, tree};
use mamba_language::application::quality_syntax::{self, PythonTarget};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::fs;

const FIXTURE: &str = include_str!("ast_renderer_controls.json");
const PROPOSAL_SHA256: &str = "66a29d913e8a13c0debc4b5f487f08048df9b69ee0bca38338430d66597869dd";
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

fn emit_evidence(variable: &str, evidence: &Value) {
    if let Some(path) = std::env::var_os(variable) {
        fs::write(
            path,
            serde_json::to_vec_pretty(evidence).expect("serialize evidence"),
        )
        .expect("write AST renderer evidence");
    }
}

#[test]
fn matches_frozen_valid_original_ast_dumps_without_counting_parse_errors_as_passes() {
    let fixture: Value = serde_json::from_str(FIXTURE).expect("valid frozen AST fixture");
    assert_eq!(
        fixture["schema"],
        "mamba.quality.black-ast-renderer-controls.v1"
    );
    assert_eq!(
        fixture["source_provenance"]["fixture_proposal_sha256"],
        PROPOSAL_SHA256
    );
    assert_eq!(fixture["source_provenance"]["source_count"], 38);
    assert_eq!(fixture["source_cases"].as_array().unwrap().len(), 38);
    assert_eq!(fixture["reference_parse_errors_are_not_comparisons"], true);

    let mut cells = Vec::new();
    let mut valid_cells = 0usize;
    let mut parse_error_cells = 0usize;
    let mut failures = 0usize;
    for case in fixture["source_cases"].as_array().unwrap() {
        let source = case["source"].as_str().expect("source text");
        let source_bytes = source.as_bytes();
        let source_sha = sha256(source_bytes);
        let source_hash_matches = case["source_sha256"].as_str() == Some(source_sha.as_str())
            && case["source_bytes"].as_u64() == Some(source_bytes.len() as u64);
        for (label, target) in TARGETS {
            let expected = &case["targets"][label];
            let status = expected["reference_status"]
                .as_str()
                .expect("reference status");
            let parsed = quality_syntax::parse(source, target, false);
            if status == "parse_error" {
                parse_error_cells += 1;
                let mut row = Map::new();
                row.insert("case".into(), case["id"].clone());
                row.insert("target".into(), json!(label));
                row.insert("source_sha256".into(), json!(source_sha));
                row.insert("source".into(), bytes_record(source_bytes));
                row.insert("status".into(), json!("reference_parse_error_not_compared"));
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
                    bytes_record(expected["traceback"].as_str().unwrap().as_bytes()),
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
                row.insert("source_hash_matches".into(), json!(source_hash_matches));
                cells.push(Value::Object(row));
                if !source_hash_matches {
                    failures += 1;
                }
                continue;
            }

            if status != "dumped" {
                failures += 1;
                cells.push(json!({
                    "case":case["id"],"target":label,"source_sha256":source_sha,
                    "status":"unknown_reference_status_failure",
                    "reference_status":status,"source_hash_matches":source_hash_matches,
                    "passed":false
                }));
                continue;
            }
            valid_cells += 1;
            let mut row = Map::new();
            row.insert("case".into(), case["id"].clone());
            row.insert("target".into(), json!(label));
            row.insert("source_sha256".into(), json!(source_sha));
            row.insert("source".into(), bytes_record(source_bytes));
            row.insert("reference_status".into(), json!(status));
            row.insert(
                "expected_tree".into(),
                bytes_record(expected["dump"].as_str().unwrap().as_bytes()),
            );
            row.insert(
                "expected_tree_sha256".into(),
                expected["dump_sha256"].clone(),
            );
            row.insert("expected_tree_root".into(), expected["root"].clone());
            let (actual_tree, outcome, issues) = match parsed {
                Err(error) => (None, "native_parse_error", json!({"parse_error":error})),
                Ok(document) if !document.is_valid() => {
                    let issue_rows = syntax_issues(&document);
                    (
                        None,
                        "native_parser_rejected_reference_valid_source",
                        json!({"syntax_issues":issue_rows}),
                    )
                }
                Ok(document) => match ast::module(&document) {
                    Err(error) => (None, "native_ast_build_error", json!({"build_error":error})),
                    Ok(module) => (
                        Some(tree::stringify(&module)),
                        "rendered",
                        json!({"syntax_issues":[]}),
                    ),
                },
            };
            row.insert("native_outcome".into(), json!(outcome));
            row.insert("native_parse_details".into(), issues);
            let matched = actual_tree.as_ref().is_some_and(|actual| {
                actual.as_bytes() == expected["dump"].as_str().unwrap().as_bytes()
            });
            if let Some(actual) = actual_tree {
                row.insert("actual_tree".into(), bytes_record(actual.as_bytes()));
            } else {
                row.insert("actual_tree".into(), Value::Null);
            }
            row.insert("tree_bytes_match".into(), json!(matched));
            row.insert("source_hash_matches".into(), json!(source_hash_matches));
            let passed = matched && source_hash_matches;
            row.insert("passed".into(), json!(passed));
            failures += usize::from(!passed);
            cells.push(Value::Object(row));
        }
    }
    let evidence = json!({
        "schema":"mamba.quality.black-ast-renderer-native-evidence.v1",
        "fixture_proposal_sha256":PROPOSAL_SHA256,
        "fixture_sha256":sha256(FIXTURE.as_bytes()),
        "cells":cells,
        "valid_ast_cells":valid_cells,
        "reference_parse_error_cells_not_compared":parse_error_cells,
        "failed_valid_cells":failures,
        "whole_acceptance":false
    });
    emit_evidence("MAMBA_QUALITY_BLACK_AST_RENDERER_AST_EVIDENCE", &evidence);
    assert_eq!(
        valid_cells, 107,
        "every original-valid target cell must be examined"
    );
    assert_eq!(
        parse_error_cells, 7,
        "original parse errors must remain outside AST comparison"
    );
    assert_eq!(
        failures, 0,
        "frozen original AST renderer comparisons failed"
    );
}

#[test]
fn matches_frozen_black_unified_diff_controls() {
    let fixture: Value = serde_json::from_str(FIXTURE).expect("valid frozen AST fixture");
    let mut cells = Vec::new();
    let mut failures = 0usize;
    for control in fixture["diff_controls"].as_array().unwrap() {
        let a = control["a"].as_str().unwrap();
        let b = control["b"].as_str().unwrap();
        let a_hash_matches = sha256(a.as_bytes()) == control["a_sha256"].as_str().unwrap();
        let b_hash_matches = sha256(b.as_bytes()) == control["b_sha256"].as_str().unwrap();
        for (label, _) in TARGETS {
            let expected = control["targets"][label]["diff"].as_str().unwrap();
            let actual = diff::unified_named(
                a,
                b,
                control["a_name"].as_str().unwrap(),
                control["b_name"].as_str().unwrap(),
            );
            let matched = actual.as_bytes() == expected.as_bytes();
            let passed = matched && a_hash_matches && b_hash_matches;
            failures += usize::from(!passed);
            cells.push(json!({
                "control":control["id"],"target":label,
                "source_a":bytes_record(a.as_bytes()),"source_b":bytes_record(b.as_bytes()),
                "source_a_name":control["a_name"],"source_b_name":control["b_name"],
                "expected_diff":bytes_record(expected.as_bytes()),
                "actual_diff":bytes_record(actual.as_bytes()),
                "source_a_hash_matches":a_hash_matches,"source_b_hash_matches":b_hash_matches,
                "passed":passed
            }));
        }
    }
    let evidence = json!({
        "schema":"mamba.quality.black-ast-renderer-diff-native-evidence.v1",
        "fixture_proposal_sha256":PROPOSAL_SHA256,
        "fixture_sha256":sha256(FIXTURE.as_bytes()),
        "cells":cells,"failed":failures,"whole_acceptance":false
    });
    emit_evidence("MAMBA_QUALITY_BLACK_AST_RENDERER_DIFF_EVIDENCE", &evidence);
    assert_eq!(failures, 0, "frozen Black unified diff comparisons failed");
}

mod nested_debug;
