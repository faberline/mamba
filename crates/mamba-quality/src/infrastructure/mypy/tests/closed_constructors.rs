//! Fixed strict-profile closed TypedDict constructor controls from Root-qualified Mypy 2.4.0 rows.
use crate::domain::{model::CompletionStatus, ports::ProjectStore, source::PythonVersion};
use crate::infrastructure::filesystem::Filesystem;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, fs, io::Write};

const ORACLE_SHA256: &str = "62e0c796c3ac60253709b19f1c3aaa65e48171a628da3359c28958f2ba776a0a";
const FIXTURE_SHA256: &str = "052fd4dd951792c746563a16c20965463e0428588ad0d3197a884eea302da0a3";

#[test]
fn pinned_closed_typed_dict_constructor_extra_checks() {
    let fixture_bytes = include_bytes!("closed_constructors.json");
    assert_eq!(
        format!("{:x}", Sha256::digest(fixture_bytes)),
        FIXTURE_SHA256
    );
    let fixture: Value = serde_json::from_slice(fixture_bytes).unwrap();
    assert_eq!(
        fixture["schema"],
        "mamba.quality.mypy.closed-typed-dict-constructor-controls.v1"
    );
    assert_eq!(fixture["oracle_sha256"], ORACLE_SHA256);
    assert_eq!(fixture["selected_flag_state"], "extra_checks_enabled");
    assert_eq!(
        fixture["counterfactual_flag_state_excluded"],
        "extra_checks_removed"
    );
    assert!(!fixture["whole_acceptance"].as_bool().unwrap());

    let cells = fixture["cells"].as_array().unwrap();
    assert_eq!(cells.len(), 12);
    let mut identities = BTreeSet::new();
    let mut case_names = BTreeSet::new();
    let mut failures = 0_usize;
    let mut evidence_rows = Vec::with_capacity(cells.len());

    for cell in cells {
        assert_eq!(cell["flag_state"], "extra_checks_enabled");
        let case_name = cell["case"].as_str().unwrap();
        case_names.insert(case_name.to_owned());
        let target = cell["target"].as_str().unwrap();
        let identity = format!("{case_name}/{target}");
        assert!(identities.insert(identity));
        let version = match target {
            "3.12" => PythonVersion::Py312,
            "3.13" => PythonVersion::Py313,
            "3.14" => PythonVersion::Py314,
            other => panic!("Unexpected target in frozen fixture: {other}"),
        };
        let source = cell["source"].as_str().unwrap();
        let source_bytes = source.as_bytes();
        let source_hash = format!("{:x}", Sha256::digest(source_bytes));
        assert_eq!(cell["source_sha256"].as_str(), Some(source_hash.as_str()));
        assert_eq!(
            cell["source_bytes"].as_u64(),
            Some(source_bytes.len() as u64)
        );
        let source_hex = hex(source_bytes);
        assert_eq!(cell["source_bytes_hex"].as_str(), Some(source_hex.as_str()));
        assert_eq!(cell["filename"], "main.py");
        let original_exit = cell["original_actual_exit"].as_i64().unwrap();
        assert!(original_exit == 0 || original_exit == 1);

        let root = tempfile::tempdir().unwrap();
        let root_path = fs::canonicalize(root.path()).unwrap();
        let path = root_path.join("main.py");
        fs::write(&path, source_bytes).unwrap();
        let before = fs::read(&path).unwrap();
        let mut workspace = Filesystem.load(std::slice::from_ref(&path), true);
        assert_eq!(workspace.projects.len(), 1);
        workspace.projects[0].python_version = version;
        let report = super::super::check(&workspace);

        let actual: Vec<_> = report
            .diagnostics
            .iter()
            .map(|diagnostic| {
                json!({
                    "rule": diagnostic.rule,
                    "message": diagnostic.message,
                    "hint": diagnostic.hint,
                    "path": diagnostic.path.strip_prefix(&root_path).unwrap(),
                    "severity": diagnostic.severity,
                    "location": {
                        "line": diagnostic.location.line,
                        "column": diagnostic.location.column,
                        "end_line": diagnostic.location.end_line,
                        "end_column": diagnostic.location.end_column,
                    },
                })
            })
            .collect();
        let actual = Value::Array(actual);
        let expected = &cell["expected_native_diagnostics"];
        let diagnostics_match = actual == *expected;
        let computed_diagnostic_status = i32::from(
            report
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.severity == "error"),
        );
        let original_exit_matches = i64::from(computed_diagnostic_status) == original_exit;
        let contract_matches = report.completion == CompletionStatus::Incomplete
            && report.reference_exit_code.is_none()
            && report.checked_files == 1;
        let after = fs::read(&path).unwrap();
        let source_preserved = before == source_bytes && after == source_bytes;
        let matched =
            diagnostics_match && original_exit_matches && contract_matches && source_preserved;
        failures += usize::from(!matched);

        evidence_rows.push(json!({
            "case": case_name,
            "target": target,
            "flag_state": cell["flag_state"],
            "source_sha256": source_hash,
            "source_path": path,
            "source_before_hex": hex(&before),
            "source_after_hex": hex(&after),
            "source_preserved": source_preserved,
            "original_actual_exit": original_exit,
            "original_json_diagnostics": cell["original_json_diagnostics"],
            "original_text_lines": cell["original_text_lines"],
            "original_raw_call_ids": cell["original_raw_call_ids"],
            "expected_native_diagnostics": expected,
            "actual_diagnostics": actual,
            "computed_diagnostic_status": computed_diagnostic_status,
            "original_exit_matches": original_exit_matches,
            "actual_completion": report.completion,
            "actual_reference_exit_code": report.reference_exit_code,
            "actual_checked_files": report.checked_files,
            "actual_tool_report": serde_json::to_value(&report).unwrap(),
            "diagnostics_match": diagnostics_match,
            "contract_matches": contract_matches,
            "matched": matched,
        }));
    }
    assert_eq!(identities.len(), 12);
    assert_eq!(
        case_names,
        BTreeSet::from([
            "closed_explicit_key".to_owned(),
            "closed_matching_source".to_owned(),
            "closed_open_extra".to_owned(),
            "closed_optional_open_interaction".to_owned(),
        ])
    );
    assert_eq!(evidence_rows.len(), 12);
    if let Some(destination) = std::env::var_os("MAMBA_QUALITY_MYPY_CLOSED_CONSTRUCTORS_EVIDENCE") {
        let mut output = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(destination)
            .unwrap();
        output
            .write_all(
                &serde_json::to_vec_pretty(&json!({
                    "schema": "mamba.quality.mypy.closed-typed-dict-constructor-comparison.v1",
                    "oracle_sha256": ORACLE_SHA256,
                    "fixture_sha256": FIXTURE_SHA256,
                    "cells": evidence_rows,
                    "failed": failures,
                    "native_reference_complete": false,
                    "whole_acceptance": false,
                }))
                .unwrap(),
            )
            .unwrap();
    }
    assert_eq!(
        failures, 0,
        "Frozen closed TypedDict constructor diagnostics did not match"
    );
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
