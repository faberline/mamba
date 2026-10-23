//! Fixed tuple-assignment-order controls from Root-qualified original captures.
use crate::domain::{model::CompletionStatus, ports::ProjectStore, source::PythonVersion};
use crate::infrastructure::filesystem::Filesystem;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fs;

fn decode_hex(text: &str) -> Vec<u8> {
    assert_eq!(text.len() % 2, 0);
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
fn tuple_assignment_order_matches_original_diagnostics() {
    let fixture_bytes = include_bytes!("tuple_assignment_order.json");
    assert_eq!(
        format!("{:x}", Sha256::digest(fixture_bytes)),
        "94b7c98358b6ed9a751a4502ca3d50930bb9f3c2237469a0650cf095caf3d091",
    );
    let fixture: Value = serde_json::from_slice(fixture_bytes).unwrap();
    assert_eq!(
        fixture["schema"],
        "mamba.quality.mypy-tuple-assignment-order.v1"
    );
    assert_eq!(fixture["qualified_original_calls"], 60);
    assert_eq!(fixture["qualified_repeat_pairs"], 30);
    assert_eq!(fixture["qualified_checks"], 120149);
    assert_eq!(fixture["historical_314_evidence_corrected"], false);
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 15);

    let mut identities = HashSet::new();
    let mut results = Vec::new();
    let mut failures = 0usize;
    for case in cases {
        let name = case["case"].as_str().unwrap();
        let target = case["target"].as_str().unwrap();
        assert!(identities.insert((target.to_owned(), name.to_owned())));
        let source = case["source_utf8"].as_str().unwrap();
        let source_bytes = decode_hex(case["source_bytes_hex"].as_str().unwrap());
        assert_eq!(source_bytes, source.as_bytes());
        assert_eq!(
            source_bytes.len(),
            case["source_bytes"].as_u64().unwrap() as usize
        );
        assert_eq!(
            format!("{:x}", Sha256::digest(&source_bytes)),
            case["source_sha256"].as_str().unwrap(),
        );
        for reporter in ["json", "text"] {
            let report = &case["original_reporters"][reporter];
            let raw_stdout = decode_hex(report["raw_stdout_hex"].as_str().unwrap());
            assert_eq!(
                format!("{:x}", Sha256::digest(&raw_stdout)),
                report["raw_stdout_sha256"].as_str().unwrap(),
            );
            let raw_stderr = decode_hex(report["raw_stderr_hex"].as_str().unwrap());
            assert_eq!(
                format!("{:x}", Sha256::digest(&raw_stderr)),
                report["raw_stderr_sha256"].as_str().unwrap(),
            );
            let repeats = report["repeat_cells"].as_array().unwrap();
            assert_eq!(repeats.len(), 2);
            assert_eq!(
                repeats[0]["raw_stdout_sha256"],
                repeats[1]["raw_stdout_sha256"]
            );
            assert_eq!(repeats[0]["actual_exit"], repeats[1]["actual_exit"]);
            assert_eq!(report["actual_original_exit"], repeats[0]["actual_exit"]);
        }

        let root = tempfile::tempdir().unwrap();
        let root_path = fs::canonicalize(root.path()).unwrap();
        let path = root_path.join(case["filename"].as_str().unwrap());
        fs::write(&path, &source_bytes).unwrap();
        let before = fs::read(&path).unwrap();
        let mut workspace = Filesystem.load(std::slice::from_ref(&path), true);
        workspace.projects[0].python_version = match target {
            "3.12" => PythonVersion::Py312,
            "3.13" => PythonVersion::Py313,
            "3.14" => PythonVersion::Py314,
            other => panic!("unexpected fixed target {other}"),
        };

        let report = super::super::check(&workspace);
        let actual_diagnostics: Vec<_> = report
            .diagnostics
            .iter()
            .map(|diagnostic| {
                json!({
                    "rule": serde_json::to_value(diagnostic).unwrap()["rule"],
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
            && report.checked_files == 1;
        let after = fs::read(&path).unwrap();
        let source_preserved = before == source_bytes && after == before;
        let native_error_count = report
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.severity == "error")
            .count();
        let matched = diagnostics_match && contract_matches && source_preserved;
        failures += usize::from(!matched);
        results.push(json!({
            "target": target,
            "case": name,
            "source_sha256": case["source_sha256"],
            "source_before_hex": before.iter().map(|byte| format!("{byte:02x}")).collect::<String>(),
            "source_after_hex": after.iter().map(|byte| format!("{byte:02x}")).collect::<String>(),
            "source_preserved": source_preserved,
            "expected_diagnostics": case["expected_diagnostics"],
            "actual_diagnostics": actual_diagnostics,
            "diagnostics_match": diagnostics_match,
            "original_reporters": case["original_reporters"],
            "native_tool_report": serde_json::to_value(&report).unwrap(),
            "native_completion": report.completion,
            "native_reference_exit": report.reference_exit_code,
            "native_checked_files": report.checked_files,
            "native_remaining": report.remaining,
            "native_error_count": native_error_count,
            "contract_matches": contract_matches,
            "matched": matched,
        }));
    }
    assert_eq!(identities.len(), 15);
    if let Some(destination) =
        std::env::var_os("MAMBA_QUALITY_MYPY_TUPLE_ASSIGNMENT_ORDER_EVIDENCE")
    {
        fs::write(
            destination,
            serde_json::to_vec_pretty(&json!({
                "schema": "mamba.quality.mypy-tuple-assignment-order-comparison.v1",
                "fixture_sha256": format!("{:x}", Sha256::digest(fixture_bytes)),
                "cases": results,
                "failed": failures,
                "whole_acceptance": false,
            }))
            .unwrap(),
        )
        .unwrap();
    }
    assert_eq!(
        failures, 0,
        "Tuple-assignment-order diagnostics differ from fixed originals"
    );
}
