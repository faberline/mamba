//! Fixed chained-assignment controls from Root-qualified mypy originals.
use crate::domain::{model::CompletionStatus, ports::ProjectStore, source::PythonVersion};
use crate::infrastructure::filesystem::Filesystem;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{collections::HashSet, fs, io::Write};

fn decode_hex(text: &str) -> Vec<u8> {
    assert!(text.len().is_multiple_of(2));
    text.as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let high = (pair[0] as char).to_digit(16).unwrap();
            let low = (pair[1] as char).to_digit(16).unwrap();
            ((high << 4) | low) as u8
        })
        .collect()
}

#[test]
fn chained_assignment_controls_match_fixed_originals() {
    let fixture_bytes = include_bytes!("chained_assignment_controls.json");
    let fixture_hash = format!("{:x}", Sha256::digest(fixture_bytes));
    assert_eq!(
        fixture_hash,
        "0bf37a94d222ad4fdc4888b9e0659fe504089a73fdd26b0b6f10eefbf57bfb85"
    );
    let fixture: Value = serde_json::from_slice(fixture_bytes).unwrap();
    assert_eq!(
        fixture["schema"],
        "mamba.quality.mypy-chained-assignment-controls.v1"
    );
    assert_eq!(
        fixture["source_manifest_sha256"],
        "5555084159258b587d6ac53f5adde94b2af19024f5fa1c265c4f6d4ffebbef93"
    );
    assert_eq!(
        fixture["capture_manifest_sha256"],
        "b68af9841a5d818b1a2fbc493cfcb6bc674d045cdeb54c6d9cd22130bf13b7c3"
    );
    assert_eq!(
        fixture["independent_original_proof_sha256"],
        "f35c5b505fe76f3aad0b8a2a13eb018a5ce100ab4993cd1c708c7b96cdb93943"
    );
    assert_eq!(
        fixture["qualified_public_oracles_sha256"],
        "d98d688a9913ffb3a29b097ac7b7f5aa98730c0a02dea2d18f3b0ea102ea82f7"
    );
    assert_eq!(fixture["qualified_original_calls"], 72);
    assert_eq!(fixture["qualified_repeat_pairs"], 36);
    assert_eq!(fixture["qualified_checks"], 135865);
    assert_eq!(fixture["whole_acceptance"], false);
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 18);

    let expected_remaining = vec![
        "mypy full strict inference is incomplete: symbol resolution, callable-value types, unpacked arguments, coroutine and generator results, containers, generics, overloads, Protocol, TypedDict, ParamSpec, TypeVarTuple, narrowing, inheritance, dataclass, and cross-file constraints",
        "generic type aliases remain incomplete: recursive expansion, complete scopes, forward references, nested binders, ParamSpec, and TypeVarTuple",
        "Pydantic 2.13.5 model construction, aliases, generic substitution, frozen fields, defaults, and dynamic-alias warnings remain incomplete",
    ];
    let mut identities = HashSet::new();
    let mut results = Vec::new();
    let mut failures = 0usize;
    for case in cases {
        let case_name = case["case"].as_str().unwrap();
        let target = case["target"].as_str().unwrap();
        assert!(identities.insert((target.to_owned(), case_name.to_owned())));
        let source = case["source_utf8"].as_str().unwrap();
        let source_bytes = decode_hex(case["source_bytes_hex"].as_str().unwrap());
        assert_eq!(source_bytes, source.as_bytes());
        assert_eq!(
            source_bytes.len(),
            case["source_bytes"].as_u64().unwrap() as usize
        );
        assert_eq!(
            format!("{:x}", Sha256::digest(&source_bytes)),
            case["source_sha256"].as_str().unwrap()
        );

        let original = &case["original_reporters"];
        for reporter in ["json", "text"] {
            let record = &original[reporter];
            let raw_stdout = decode_hex(record["raw_stdout_hex"].as_str().unwrap());
            let raw_stderr = decode_hex(record["raw_stderr_hex"].as_str().unwrap());
            assert_eq!(
                format!("{:x}", Sha256::digest(&raw_stdout)),
                record["raw_stdout_sha256"].as_str().unwrap()
            );
            assert_eq!(
                format!("{:x}", Sha256::digest(&raw_stderr)),
                record["raw_stderr_sha256"].as_str().unwrap()
            );
            let repeats = record["repeat_cells"].as_array().unwrap();
            assert_eq!(repeats.len(), 2);
            for repeat in repeats {
                let repeat_stdout = decode_hex(repeat["raw_stdout_hex"].as_str().unwrap());
                let repeat_stderr = decode_hex(repeat["raw_stderr_hex"].as_str().unwrap());
                assert_eq!(
                    format!("{:x}", Sha256::digest(&repeat_stdout)),
                    repeat["raw_stdout_sha256"].as_str().unwrap()
                );
                assert_eq!(
                    format!("{:x}", Sha256::digest(&repeat_stderr)),
                    repeat["raw_stderr_sha256"].as_str().unwrap()
                );
                assert_eq!(repeat["actual_exit"], record["original_exit"]);
                assert_eq!(repeat["raw_stdout_hex"], record["raw_stdout_hex"]);
                assert_eq!(repeat["raw_stderr_hex"], record["raw_stderr_hex"]);
            }
            assert_eq!(record["original_exit"], 1);
        }
        if case_name == "typed_context_anchor" {
            let assignment = case["expected_diagnostics"]
                .as_array()
                .unwrap()
                .iter()
                .find(|diagnostic| diagnostic["rule"] == "assignment")
                .unwrap();
            assert_eq!(assignment["hint"], "\"list\" is invariant -- see https://mypy.readthedocs.io/en/stable/common_issues.html#variance\nConsider using \"Sequence\" instead, which is covariant");
        }

        let version = match target {
            "3.12" => PythonVersion::Py312,
            "3.13" => PythonVersion::Py313,
            "3.14" => PythonVersion::Py314,
            other => panic!("unexpected frozen target {other}"),
        };
        let root = tempfile::tempdir().unwrap();
        let root_path = fs::canonicalize(root.path()).unwrap();
        let path = root_path.join(case["filename"].as_str().unwrap());
        fs::write(&path, &source_bytes).unwrap();
        let before = fs::read(&path).unwrap();
        let mut workspace = Filesystem.load(std::slice::from_ref(&path), true);
        workspace.projects[0].python_version = version;
        let report = super::super::check(&workspace);
        let actual_diagnostics: Vec<_> = report
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
        let actual_diagnostics = Value::Array(actual_diagnostics);
        let diagnostics_match = actual_diagnostics == case["expected_diagnostics"];
        let contract_matches = report.completion == CompletionStatus::Incomplete
            && report.reference_exit_code.is_none()
            && report.checked_files == 1
            && report.remaining == expected_remaining;
        let after = fs::read(&path).unwrap();
        let source_preserved = before == source_bytes && after == before;
        let computed_diagnostic_status =
            i32::from(report.diagnostics.iter().any(|d| d.severity == "error"));
        let original_exits_match = original["json"]["original_exit"].as_i64()
            == Some(i64::from(computed_diagnostic_status))
            && original["text"]["original_exit"].as_i64()
                == Some(i64::from(computed_diagnostic_status));
        let matched =
            diagnostics_match && contract_matches && source_preserved && original_exits_match;
        failures += usize::from(!matched);
        results.push(json!({
            "case": case_name,
            "target": target,
            "source_sha256": case["source_sha256"],
            "source_before_hex": before.iter().map(|byte| format!("{byte:02x}")).collect::<String>(),
            "source_after_hex": after.iter().map(|byte| format!("{byte:02x}")).collect::<String>(),
            "source_preserved": source_preserved,
            "expected_diagnostics": case["expected_diagnostics"],
            "actual_diagnostics": actual_diagnostics,
            "diagnostics_match": diagnostics_match,
            "original_reporters": original,
            "native_tool_report": serde_json::to_value(&report).unwrap(),
            "computed_diagnostic_status": computed_diagnostic_status,
            "json_original_exit": original["json"]["original_exit"],
            "text_original_exit": original["text"]["original_exit"],
            "original_exits_match": original_exits_match,
            "native_completion": report.completion,
            "native_reference_exit": report.reference_exit_code,
            "native_checked_files": report.checked_files,
            "native_remaining": report.remaining,
            "contract_matches": contract_matches,
            "matched": matched,
        }));
    }
    assert_eq!(identities.len(), 18);
    if let Some(destination) =
        std::env::var_os("MAMBA_QUALITY_MYPY_CHAINED_ASSIGNMENT_CONTROLS_EVIDENCE")
    {
        let mut output = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(destination)
            .unwrap();
        output
            .write_all(
                &serde_json::to_vec_pretty(&json!({
                    "schema": "mamba.quality.mypy-chained-assignment-controls-comparison.v1",
                    "fixture_sha256": fixture_hash,
                    "cells": results,
                    "failed": failures,
                    "whole_acceptance": false,
                }))
                .unwrap(),
            )
            .unwrap();
    }
    assert_eq!(
        failures, 0,
        "Chained-assignment diagnostics differ from fixed originals"
    );
}
