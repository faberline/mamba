//! Frozen callable diagnostics from original Mypy JSON and full-position text output.
use crate::domain::{model::CompletionStatus, ports::ProjectStore, source::PythonVersion};
use crate::infrastructure::filesystem::Filesystem;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs;

#[test]
fn pinned_original_callable_diagnostics() {
    let corpus = include_str!("callable_originals.json");
    assert_eq!(
        format!("{:x}", Sha256::digest(corpus.as_bytes())),
        "20e83b2a158050e85b489ceed986ec6554fccccb10244e0d30b7d9dcb7a36ce5"
    );
    let parsed_corpus: Value = serde_json::from_str(corpus).unwrap();
    let cases: Vec<Value> = serde_json::from_value(parsed_corpus["cases"].clone()).unwrap();
    assert_eq!(cases.len(), 6);

    let mut results = Vec::new();
    let mut failures = 0;
    for case in cases {
        let source = case["source"].as_str().unwrap();
        let source_hash = format!("{:x}", Sha256::digest(source.as_bytes()));
        assert_eq!(source_hash, case["source_sha256"].as_str().unwrap());
        let root = tempfile::tempdir().unwrap();
        let root_path = fs::canonicalize(root.path()).unwrap();
        let path = root_path.join(case["filename"].as_str().unwrap());
        fs::write(&path, source.as_bytes()).unwrap();
        let source_before = fs::read(&path).unwrap();
        let source_before_hash = format!("{:x}", Sha256::digest(&source_before));
        let source_input_matches_fixture = source_before == source.as_bytes();
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
            let actual = Value::Array(actual_diagnostics);
            let expected = &case["expected"][target];
            let diagnostics_equal = actual == *expected;
            let diagnostic_status = i32::from(
                report
                    .diagnostics
                    .iter()
                    .any(|diagnostic| diagnostic.severity == "error"),
            );
            let reference_text_process_exit = case["reference_text_process_exits"][target]
                .as_i64()
                .unwrap() as i32;
            let original_json_process_exit = case["original_json_process_exits"][target]
                .as_i64()
                .unwrap() as i32;
            let text_exit_matches_diagnostic_status =
                reference_text_process_exit == diagnostic_status;
            let completion_is_expected = report.completion == CompletionStatus::Incomplete;
            let reference_exit_is_unknown = report.reference_exit_code.is_none();
            let one_file_was_checked = report.checked_files == 1;
            let bytes_after = fs::read(&path).unwrap();
            let source_after_hash = format!("{:x}", Sha256::digest(&bytes_after));
            let source_preserved = source_input_matches_fixture && bytes_after == source_before;

            let cell_matches = diagnostics_equal
                && text_exit_matches_diagnostic_status
                && completion_is_expected
                && reference_exit_is_unknown
                && one_file_was_checked
                && source_preserved;
            failures += usize::from(!cell_matches);
            results.push(json!({
                "name": case["name"],
                "source": source,
                "source_sha256": source_hash,
                "python": version.name(),
                "expected_diagnostics": expected,
                "actual_diagnostics": actual,
                "diagnostics_equal": diagnostics_equal,
                "reference_text_process_exit": reference_text_process_exit,
                "original_json_process_exit": original_json_process_exit,
                "computed_diagnostic_status": diagnostic_status,
                "text_exit_matches_computed_diagnostic_status": text_exit_matches_diagnostic_status,
                "actual_completion": report.completion,
                "completion_is_expected_incomplete": completion_is_expected,
                "actual_reference_exit_code": report.reference_exit_code,
                "reference_exit_remains_unknown": reference_exit_is_unknown,
                "actual_checked_files": report.checked_files,
                "one_file_was_checked": one_file_was_checked,
                "source_sha256_before": source_before_hash,
                "source_sha256_after": source_after_hash,
                "source_before_matches_fixture": source_input_matches_fixture,
                "source_preserved": source_preserved,
                "json_stdout_sha256": case["original_reporter_refs"][target]["json"]["stdout_sha256"],
                "text_stdout_sha256": case["original_reporter_refs"][target]["text"]["stdout_sha256"],
                "original_json_messages": case["original_reporter_refs"][target]["json"]["raw_json_messages"],
                "original_text_lines": case["original_reporter_refs"][target]["text"]["raw_lines"],
                "cell_matches": cell_matches,
            }));
        }
    }

    assert_eq!(results.len(), 18);
    if let Some(destination) = std::env::var_os("MAMBA_QUALITY_MYPY_CALLABLE_NATIVE_EVIDENCE") {
        fs::write(
            destination,
            serde_json::to_vec_pretty(&json!({
                "schema": "mamba.quality.mypy-callable-original-comparison.v1",
                "fixture_sha256": format!("{:x}", Sha256::digest(corpus.as_bytes())),
                "cases": results,
                "failed": failures,
                "native_reference_complete": false,
                "whole_som_accepted": false,
            }))
            .unwrap(),
        )
        .unwrap();
    }
    assert_eq!(failures, 0, "Frozen callable diagnostics did not match");
}
