//! Fixed tuple-target diagnostics from independently qualified original mypy captures.
use crate::domain::{model::CompletionStatus, ports::ProjectStore, source::PythonVersion};
use crate::infrastructure::filesystem::Filesystem;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fs;

#[test]
fn tuple_binding_controls_match_original_diagnostics() {
    let fixture_bytes = include_bytes!("tuple_binding_controls.json");
    assert_eq!(
        format!("{:x}", Sha256::digest(fixture_bytes)),
        "a9034022bd960d7923d7dfe8b23311d46f6d073570f8343dab8b6457f2e4c233",
    );
    let fixture: Value = serde_json::from_slice(fixture_bytes).unwrap();
    assert_eq!(
        fixture["schema"],
        "mamba.quality.mypy-tuple-binding-controls.v1"
    );
    assert_eq!(fixture["captured_original_calls"], 36);
    assert_eq!(fixture["original_repeat_pairs"], 18);
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 9);

    let mut identities = HashSet::new();
    let mut results = Vec::new();
    let mut failures = 0usize;
    for case in cases {
        let name = case["case"].as_str().unwrap();
        let target = case["target"].as_str().unwrap();
        assert!(identities.insert((target.to_owned(), name.to_owned())));
        let source = case["source_utf8"].as_str().unwrap();
        assert_eq!(
            source.len(),
            case["source_bytes"].as_u64().unwrap() as usize
        );
        let source_hex: String = source
            .as_bytes()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        assert_eq!(source_hex, case["source_bytes_hex"].as_str().unwrap());
        assert_eq!(
            format!("{:x}", Sha256::digest(source.as_bytes())),
            case["source_sha256"].as_str().unwrap(),
        );

        let root = tempfile::tempdir().unwrap();
        let root_path = fs::canonicalize(root.path()).unwrap();
        let path = root_path.join(case["filename"].as_str().unwrap());
        fs::write(&path, source.as_bytes()).unwrap();
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
        let source_preserved = before == source.as_bytes() && after == before;
        let matched = diagnostics_match && contract_matches && source_preserved;
        failures += usize::from(!matched);
        results.push(json!({
            "target": target,
            "case": name,
            "source_sha256": case["source_sha256"],
            "source_before_hex": source_hex,
            "source_after_hex": after.iter().map(|byte| format!("{byte:02x}")).collect::<String>(),
            "source_preserved": source_preserved,
            "expected_diagnostics": case["expected_diagnostics"],
            "actual_diagnostics": actual_diagnostics,
            "diagnostics_match": diagnostics_match,
            "original_process": {
                "json_exit": case["json_original_exit"],
                "json_raw_stdout_sha256": case["json_raw_stdout_sha256"],
                "text_exit": case["text_original_exit"],
                "text_raw_stdout_sha256": case["text_raw_stdout_sha256"],
            },
            "native_tool_report": serde_json::to_value(&report).unwrap(),
            "native_completion": report.completion,
            "native_reference_exit": report.reference_exit_code,
            "native_checked_files": report.checked_files,
            "native_remaining": report.remaining,
            "contract_matches": contract_matches,
            "matched": matched,
        }));
    }
    assert_eq!(identities.len(), 9);
    if let Some(destination) =
        std::env::var_os("MAMBA_QUALITY_MYPY_TUPLE_BINDING_CONTROLS_EVIDENCE")
    {
        fs::write(
            destination,
            serde_json::to_vec_pretty(&json!({
                "schema": "mamba.quality.mypy-tuple-binding-controls-comparison.v1",
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
        "Tuple-binding diagnostics differ from fixed originals"
    );
}
