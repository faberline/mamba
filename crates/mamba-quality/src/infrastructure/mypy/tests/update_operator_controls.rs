//! Fixed strict-profile TypedDict update/operator controls from qualified mypy 2.4.0 originals.
use crate::domain::{model::CompletionStatus, ports::ProjectStore, source::PythonVersion};
use crate::infrastructure::filesystem::Filesystem;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, fs, io::Write};

const FIXTURE_SHA256: &str = "ff47dd116e5fd25ee9ce916cce7a09b2d0055d9c75881ee56b3c0317a5978f47";
const ORACLE_SHA256: &str = "db7548c559025d12d639464636318d1bd31c5105249e38a75e410264818cf3fc";
const CAPTURE_SHA256: &str = "bd175762877e08b6c6bbfba8d082b171d9f4da9fc1fd40984a8e33773e2933eb";
const PROOF_SHA256: &str = "3cb5487c273a6fe933577eb30e4a953928f45ca2cbd56951395a6146a0849d08";
const EXPECTED_REMAINING: [&str; 3] = [
    "mypy full strict inference is incomplete: symbol resolution, callable-value types, unpacked arguments, coroutine and generator results, containers, generics, overloads, Protocol, TypedDict, ParamSpec, TypeVarTuple, narrowing, inheritance, dataclass, and cross-file constraints",
    "generic type aliases remain incomplete: recursive expansion, complete scopes, forward references, nested binders, ParamSpec, and TypeVarTuple",
    "Pydantic 2.13.5 model construction, aliases, generic substitution, frozen fields, defaults, and dynamic-alias warnings remain incomplete",
];

#[test]
fn pinned_typeddict_update_and_operator_controls() {
    let fixture_bytes = include_bytes!("update_operator_controls.json");
    assert_eq!(sha256(fixture_bytes), FIXTURE_SHA256);
    let fixture: Value = serde_json::from_slice(fixture_bytes).unwrap();
    assert_eq!(
        fixture["schema"],
        "mamba.quality.mypy.typeddict-update-operator-controls.v1"
    );
    assert_eq!(fixture["oracle_sha256"], ORACLE_SHA256);
    assert_eq!(fixture["capture_manifest_sha256"], CAPTURE_SHA256);
    assert_eq!(fixture["qualification_proof_sha256"], PROOF_SHA256);
    assert_eq!(fixture["selected_flag_state"], "extra_checks_enabled");
    assert_eq!(fixture["excluded_flag_state"], "extra_checks_removed");
    assert_eq!(fixture["strict_product_cells"], 24);
    assert_eq!(fixture["original_calls_for_selected_cells"], 96);
    assert_eq!(fixture["repeat_pairs_for_selected_cells"], 48);
    assert_eq!(fixture["counterfactual_cells_excluded"], 24);
    assert_eq!(fixture["whole_acceptance"], false);

    let cells = fixture["cells"].as_array().unwrap();
    assert_eq!(cells.len(), 24);
    let mut identities = BTreeSet::new();
    let mut case_names = BTreeSet::new();
    let mut failures = 0usize;
    let mut evidence_rows = Vec::with_capacity(cells.len());

    for cell in cells {
        assert_eq!(cell["flag_state"], "extra_checks_enabled");
        let case_name = cell["case"].as_str().unwrap();
        case_names.insert(case_name.to_owned());
        let target = cell["target"].as_str().unwrap();
        assert!(identities.insert((case_name.to_owned(), target.to_owned())));
        let version = match target {
            "3.12" => PythonVersion::Py312,
            "3.13" => PythonVersion::Py313,
            "3.14" => PythonVersion::Py314,
            other => panic!("unexpected target in frozen fixture: {other}"),
        };
        let source = cell["source"].as_str().unwrap();
        let source_bytes = source.as_bytes();
        let source_hash = sha256(source_bytes);
        assert_eq!(cell["filename"], "main.py");
        assert_eq!(cell["source_sha256"].as_str(), Some(source_hash.as_str()));
        assert_eq!(
            cell["source_bytes"].as_u64(),
            Some(source_bytes.len() as u64)
        );
        assert_eq!(
            cell["source_bytes_hex"].as_str(),
            Some(hex(source_bytes).as_str())
        );

        let raw_calls = cell["raw_original_calls"].as_array().unwrap();
        assert_eq!(raw_calls.len(), 4);
        for reporter in ["json", "text"] {
            let pair: Vec<_> = raw_calls
                .iter()
                .filter(|call| call["reporter"] == reporter)
                .collect();
            assert_eq!(pair.len(), 2);
            assert_eq!(pair[0]["repeat"], 1);
            assert_eq!(pair[1]["repeat"], 2);
            assert_eq!(pair[0]["actual_exit"], pair[1]["actual_exit"]);
            assert_eq!(pair[0]["stdout_hex"], pair[1]["stdout_hex"]);
            assert_eq!(pair[0]["stderr_hex"], pair[1]["stderr_hex"]);
            let expected_exit = if reporter == "json" {
                cell["original_json_exit"].as_i64().unwrap()
            } else {
                cell["original_text_exit"].as_i64().unwrap()
            };
            assert_eq!(pair[0]["actual_exit"].as_i64(), Some(expected_exit));
            assert!(pair.iter().all(|call| call["qualified"] == true));
        }
        assert!(raw_calls.iter().all(|call| call["id"].is_string()
            && call["stdout_sha256"].is_string()
            && call["stderr_sha256"].is_string()
            && call["process_record_sha256"].is_string()
            && call["checker_imports_sha256"].is_string()));

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
        assert_eq!(
            cell["native_error_status_projection"],
            cell["original_text_exit"]
        );
        let computed_error_status = i32::from(
            report
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.severity == "error"),
        );
        let native_status_matches = computed_error_status
            == cell["native_error_status_projection"].as_i64().unwrap() as i32;
        let text_exit_matches =
            computed_error_status == cell["original_text_exit"].as_i64().unwrap() as i32;
        let json_exit_recorded = cell["original_json_exit"].as_i64().is_some();
        let contract_matches = report.completion == CompletionStatus::Incomplete
            && report.reference_exit_code.is_none()
            && report.checked_files == 1
            && report
                .remaining
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>()
                == EXPECTED_REMAINING.to_vec();
        let after = fs::read(&path).unwrap();
        let source_preserved = before == source_bytes && after == source_bytes;
        let matched = diagnostics_match
            && native_status_matches
            && text_exit_matches
            && json_exit_recorded
            && contract_matches
            && source_preserved;
        failures += usize::from(!matched);

        evidence_rows.push(json!({
            "case": case_name,
            "target": target,
            "flag_state": cell["flag_state"],
            "operation_method": cell["operation_method"],
            "mutating_hook": cell["mutating_hook"],
            "source_path": path,
            "source_sha256": source_hash,
            "source_before_hex": hex(&before),
            "source_after_hex": hex(&after),
            "source_preserved": source_preserved,
            "original_json_exit": cell["original_json_exit"],
            "original_text_exit": cell["original_text_exit"],
            "native_error_status_projection": cell["native_error_status_projection"],
            "original_json_diagnostics": cell["json_diagnostics"],
            "original_text_lines": cell["text_lines"],
            "raw_original_calls": raw_calls,
            "expected_native_diagnostics": expected,
            "actual_diagnostics": actual,
            "computed_error_status": computed_error_status,
            "native_status_matches": native_status_matches,
            "text_exit_matches": text_exit_matches,
            "original_json_exit_is_provenance_only": json_exit_recorded,
            "native_tool_report": serde_json::to_value(&report).unwrap(),
            "native_completion": report.completion,
            "native_reference_exit_code": report.reference_exit_code,
            "native_checked_files": report.checked_files,
            "native_remaining": report.remaining,
            "diagnostics_match": diagnostics_match,
            "contract_matches": contract_matches,
            "matched": matched,
        }));
    }

    assert_eq!(identities.len(), 24);
    assert_eq!(
        case_names,
        BTreeSet::from([
            "optional_update".to_owned(),
            "partial_update_control".to_owned(),
            "readonly_inplace".to_owned(),
            "readonly_ior".to_owned(),
            "readonly_or".to_owned(),
            "readonly_ror".to_owned(),
            "readonly_update".to_owned(),
            "union_update".to_owned(),
        ])
    );
    assert_eq!(evidence_rows.len(), 24);
    if let Some(destination) =
        std::env::var_os("MAMBA_QUALITY_MYPY_UPDATE_OPERATOR_CONTROLS_EVIDENCE")
    {
        let mut output = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(destination)
            .unwrap();
        output
            .write_all(
                &serde_json::to_vec_pretty(&json!({
                    "schema": "mamba.quality.mypy.typeddict-update-operator-comparison.v1",
                    "fixture_sha256": sha256(fixture_bytes),
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
        "Frozen TypedDict update/operator diagnostics differ from originals"
    );
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
