//! Protected TypedDict union-boundary controls from qualified Mypy 2.4.0 runs.
use crate::domain::{model::CompletionStatus, ports::ProjectStore, source::PythonVersion};
use crate::infrastructure::filesystem::Filesystem;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, fs, io::Write};

const FIXTURE_SHA256: &str = "aeb965392b0b204db7040cf8f3e7589f021ff28ee9d74619274e9c7cf0e496e7";
const SOURCE_PACK_SHA256: &str = "20e7cec1a769774487562c5a40fe9f4c4c6b2f870f708900abdac50b7bb29213";
const SOURCES_SHA256: &str = "0a57d34418f1a2285e3aa248eff6c6c94e46cd42f69e7b466e2ab6cfb1289f50";
const CAPTURE_MANIFEST_SHA256: &str =
    "1c81b8fee0e26cb70712ffb3f323643b018fa95c37aef4895ddb3886083ff29a";
const CAPTURE_VERIFICATION_SHA256: &str =
    "42320ca3732ee0014e326010c9578b9fb4137d7c0d9ec3b66c9b0af174c939f6";
const PROFILE_FLAGS: [&str; 13] = [
    "--disallow-any-generics",
    "--disallow-subclassing-any",
    "--disallow-untyped-calls",
    "--disallow-untyped-defs",
    "--disallow-incomplete-defs",
    "--check-untyped-defs",
    "--disallow-untyped-decorators",
    "--warn-redundant-casts",
    "--warn-unused-ignores",
    "--warn-return-any",
    "--no-implicit-reexport",
    "--strict-equality",
    "--extra-checks",
];

#[test]
fn pinned_typed_dict_union_boundary_originals() {
    let fixture_bytes = include_bytes!("union_boundary.json");
    let fixture_hash = format!("{:x}", Sha256::digest(fixture_bytes));
    assert_eq!(fixture_hash, FIXTURE_SHA256);
    let fixture: Value = serde_json::from_slice(fixture_bytes).unwrap();
    assert_eq!(
        fixture["schema"],
        "mamba.quality.mypy.union-boundary-controls.v1"
    );
    assert_eq!(fixture["source_pack_artifact_sha256"], SOURCE_PACK_SHA256);
    assert_eq!(fixture["sources_sha256"], SOURCES_SHA256);
    assert_eq!(fixture["capture_manifest_sha256"], CAPTURE_MANIFEST_SHA256);
    assert_eq!(
        fixture["capture_verification_sha256"],
        CAPTURE_VERIFICATION_SHA256
    );
    assert_eq!(fixture["flag_state"], "extra_checks_enabled");
    assert_eq!(fixture["native_reference_complete"], false);
    assert_eq!(fixture["whole_acceptance"], false);

    let case_names = [
        "duplicate_receiver_partials",
        "mixed_typeddict_non_typeddict_union",
        "closed_receiver_open_source_union",
        "readonly_requiredness_subtype_partials",
    ];
    let targets = ["3.12", "3.13", "3.14"];
    let cells = fixture["cells"].as_array().unwrap();
    assert_eq!(cells.len(), 12);
    let mut identities = BTreeSet::new();
    let mut rows = Vec::with_capacity(cells.len());
    let mut failures = 0_usize;

    for cell in cells {
        let case = cell["case"].as_str().unwrap();
        let target = cell["target"].as_str().unwrap();
        let identity = format!("{case}/{target}");
        let unique = identities.insert(identity);
        let version = match target {
            "3.12" => PythonVersion::Py312,
            "3.13" => PythonVersion::Py313,
            "3.14" => PythonVersion::Py314,
            other => panic!("Unexpected target in frozen fixture: {other}"),
        };
        let source = cell["source"].as_str().unwrap().as_bytes();
        let source_before_hash = format!("{:x}", Sha256::digest(source));
        let source_hex = hex(source);
        let root = tempfile::tempdir().unwrap();
        let root_path = fs::canonicalize(root.path()).unwrap();
        let path = root_path.join("main.py");
        fs::write(&path, source).unwrap();
        let before = fs::read(&path).unwrap();
        let mut workspace = Filesystem.load(std::slice::from_ref(&path), true);
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
        let expected = cell["expected_native_diagnostics"].clone();
        let diagnostics_match = actual == expected;
        let computed_diagnostic_status = i32::from(
            report
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.severity == "error"),
        );
        let json_exit = cell["original_json"]["actual_exit"].as_i64();
        let text_exit = cell["original_text"]["actual_exit"].as_i64();
        let original_exit_matches = json_exit == Some(i64::from(computed_diagnostic_status))
            && text_exit == Some(i64::from(computed_diagnostic_status));
        let contract_matches = report.completion == CompletionStatus::Incomplete
            && report.reference_exit_code.is_none()
            && report.checked_files == 1;
        let after = fs::read(&path).unwrap();
        let source_preserved = before == source && after == source;
        let source_binding_matches = cell["filename"] == "main.py"
            && cell["flag_state"] == "extra_checks_enabled"
            && cell["profile_flags"] == json!(PROFILE_FLAGS)
            && cell["source_bytes"] == source.len()
            && cell["source_sha256"] == source_before_hash
            && cell["source_hex"] == source_hex;
        let matched = unique
            && source_binding_matches
            && diagnostics_match
            && original_exit_matches
            && contract_matches
            && source_preserved;
        failures += usize::from(!matched);
        rows.push(json!({
            "case": case,
            "target": target,
            "flag_state": cell["flag_state"],
            "source_path": path,
            "source_sha256": source_before_hash,
            "source_before_hex": hex(&before),
            "source_after_hex": hex(&after),
            "source_preserved": source_preserved,
            "source_binding_matches": source_binding_matches,
            "original_json": cell["original_json"],
            "original_text": cell["original_text"],
            "json_repeat": cell["json_repeat"],
            "text_repeat": cell["text_repeat"],
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

    let evidence = json!({
        "schema": "mamba.quality.mypy.union-boundary-comparison.v1",
        "fixture_sha256": fixture_hash,
        "source_pack_artifact_sha256": SOURCE_PACK_SHA256,
        "sources_sha256": SOURCES_SHA256,
        "capture_manifest_sha256": CAPTURE_MANIFEST_SHA256,
        "capture_verification_sha256": CAPTURE_VERIFICATION_SHA256,
        "cases": case_names,
        "targets": targets,
        "cells": rows,
        "failed": failures,
        "native_reference_complete": false,
        "whole_acceptance": false,
    });
    if let Some(destination) = std::env::var_os("MAMBA_QUALITY_MYPY_UNION_BOUNDARY_EVIDENCE") {
        let mut output = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(destination)
            .unwrap();
        output
            .write_all(&serde_json::to_vec_pretty(&evidence).unwrap())
            .unwrap();
    }

    // All per-cell rows are written before these parity assertions.
    let expected_cases: BTreeSet<String> =
        case_names.iter().map(|case| (*case).to_owned()).collect();
    let actual_cases: BTreeSet<String> = cells
        .iter()
        .filter_map(|cell| cell["case"].as_str().map(str::to_owned))
        .collect();
    assert_eq!(actual_cases, expected_cases);
    assert_eq!(identities.len(), 12);
    assert_eq!(targets.len() * case_names.len(), rows.len());
    assert_eq!(
        failures, 0,
        "Frozen Mypy union-boundary diagnostics did not match"
    );
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
