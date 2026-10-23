//! Six fixed original insertion-marker controls. No native result defines an oracle.
use super::*;
use crate::{
    domain::{ports::QualityEngine, source::PythonVersion},
    infrastructure::native::NativeEngine,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::path::Path;

const REFERENCE_SHA256: &str = "03619fe9d5f3eda3c7f972cfa4910d1c664eae0c858e5cf0664ed94ecaed2a17";
const PARITY_PENDING: &str =
    "isort full-profile reference parity has not passed the complete original corpus";

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn decode(value: &Value) -> Vec<u8> {
    let hex = value.as_str().unwrap();
    assert!(hex.len().is_multiple_of(2));
    (0..hex.len())
        .step_by(2)
        .map(|offset| u8::from_str_radix(&hex[offset..offset + 2], 16).unwrap())
        .collect()
}

// This position is the existing native imports diagnostic API. Original isort's
// CLI check output supplies a file path, but does not supply a source position.
fn diagnostics_match(report: &ToolReport, path: &Path, original_check_exit: i64) -> bool {
    match original_check_exit {
        0 => report.diagnostics.is_empty(),
        1 => {
            report.diagnostics.len() == 1
                && report.diagnostics.iter().all(|diagnostic| {
                    diagnostic.tool == QualityTool::Isort
                        && diagnostic.rule == "imports"
                        && diagnostic.message == "Imports are incorrectly sorted and/or formatted."
                        && diagnostic.hint.is_none()
                        && diagnostic.path == path
                        && diagnostic.severity == "error"
                        && diagnostic.fix.is_none()
                        && diagnostic.location
                            == QualityLocation {
                                line: 1,
                                column: 1,
                                end_line: None,
                                end_column: None,
                                offset: Some(0),
                                end_offset: None,
                            }
                })
        }
        _ => false,
    }
}

// A diagnostic status is a computed 0/1 projection. It is not a native CLI exit.
// Native completion stays Incomplete and its reference exit stays null.
fn capture(workspace: &QualityWorkspace) -> (Option<Vec<u8>>, Value, bool) {
    let profile = FixedProfile::locked();
    let report = NativeEngine.check(workspace, &profile, QualityTool::Isort);
    let transform = NativeEngine.transform(workspace, &profile, QualityTool::Isort);
    let candidate_shape = transform.candidates.is_empty()
        || (transform.candidates.len() == 1 && transform.candidates[0].0 == 0);
    let text = transform
        .candidates
        .first()
        .map_or(workspace.sources[0].text.as_str(), |(_, text)| {
            text.as_str()
        });
    let encoded = Filesystem.candidate_bytes(&workspace.sources[0], text);
    let diagnostic_status = i32::from(!report.diagnostics.is_empty());
    let api_contract = report.tool == QualityTool::Isort
        && report.reference_version == "9.0.2"
        && report.completion == CompletionStatus::Incomplete
        && report.reference_exit_code.is_none()
        && report.checked_files == 1
        && report.remaining == [PARITY_PENDING]
        && transform.completion == CompletionStatus::Incomplete
        && transform.issues.is_empty()
        && transform.remaining == [PARITY_PENDING]
        && candidate_shape;
    let evidence = json!({
        "check_report":report,
        "diagnostic_status":diagnostic_status,
        "diagnostic_status_is_process_exit":false,
        "transform":{
            "completion":transform.completion,
            "candidates":transform.candidates,
            "issues":transform.issues,
            "remaining":transform.remaining,
        },
        "candidate_shape_valid":candidate_shape,
        "candidate_encoding_error":encoded.as_ref().err(),
        "candidate_hex":encoded.as_ref().ok().map(|bytes| hex(bytes)),
        "candidate_sha256":encoded.as_ref().ok().map(|bytes| sha256(bytes)),
        "api_contract":api_contract,
    });
    (encoded.ok(), evidence, api_contract)
}

#[test]
fn pinned_original_insertion_markers() {
    let corpus = include_str!("insertion_markers.json");
    assert_eq!(sha256(corpus.as_bytes()), REFERENCE_SHA256);
    let fixture: Value = serde_json::from_str(corpus).unwrap();
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 6);
    assert_eq!(fixture["version"], "9.0.2");
    assert_eq!(fixture["profile"], "black");
    assert_eq!(fixture["line_length"], 88);
    let mut results = Vec::new();
    let mut failed = 0;
    for case in cases {
        let source = decode(&case["source_hex"]);
        assert_eq!(sha256(&source), case["source_sha256"].as_str().unwrap());
        for (target, version) in [
            PythonVersion::Py312,
            PythonVersion::Py313,
            PythonVersion::Py314,
        ]
        .into_iter()
        .enumerate()
        {
            let expected = &case["expected"][target];
            assert_eq!(expected["python"], version.name());
            let expected_first = decode(&expected["first_output_hex"]);
            let expected_stable = decode(&expected["stable_output_hex"]);
            assert_eq!(
                sha256(&expected_first),
                expected["first_output_sha256"].as_str().unwrap()
            );
            assert_eq!(
                sha256(&expected_stable),
                expected["stable_output_sha256"].as_str().unwrap()
            );
            assert_eq!(expected["first_format_exit"], 0);
            assert_eq!(expected["repeat_exit"], 0);
            assert_eq!(expected["stable"], true);
            let root = tempfile::tempdir().unwrap();
            let path = root.path().canonicalize().unwrap().join("main.py");
            fs::write(&path, &source).unwrap();
            let mut workspace = Filesystem.load(std::slice::from_ref(&path), true);
            workspace.projects[0].python_version = version;
            let (first, first_evidence, first_api) = capture(&workspace);
            let source_unchanged = fs::read(&path).unwrap() == source;
            let first_matches = first.as_ref() == Some(&expected_first);
            let source_check_exit = expected["source_check_exit"].as_i64().unwrap();
            let first_diagnostics = diagnostics_match(
                &serde_json::from_value(first_evidence["check_report"].clone()).unwrap(),
                &path,
                source_check_exit,
            );
            let byte_change_matches = first
                .as_ref()
                .is_some_and(|bytes| i64::from(i32::from(bytes != &source)) == source_check_exit);
            let mut second_evidence = Value::Null;
            let mut second_matches = false;
            let mut idempotent = false;
            let mut stable_diagnostics = false;
            let mut second_api = false;
            let mut first_candidate_unchanged = false;
            if let Some(first_bytes) = first.as_ref() {
                fs::write(&path, first_bytes).unwrap();
                let mut repeated = Filesystem.load(std::slice::from_ref(&path), true);
                repeated.projects[0].python_version = version;
                let (second, evidence, api) = capture(&repeated);
                first_candidate_unchanged = fs::read(&path).unwrap() == *first_bytes;
                second_matches = second.as_ref() == Some(&expected_stable);
                idempotent = second.as_ref() == Some(first_bytes);
                stable_diagnostics = diagnostics_match(
                    &serde_json::from_value(evidence["check_report"].clone()).unwrap(),
                    &path,
                    expected["stable_check_exit"].as_i64().unwrap(),
                );
                second_api = api;
                second_evidence = evidence;
            }
            let passed = first_matches
                && second_matches
                && idempotent
                && first_diagnostics
                && stable_diagnostics
                && byte_change_matches
                && first_api
                && second_api
                && source_unchanged
                && first_candidate_unchanged;
            failed += usize::from(!passed);
            results.push(json!({
                "id":case["id"],"python":version.name(),"path":path,
                "source_hex":case["source_hex"],"source_sha256":case["source_sha256"],
                "original_reference":expected,
                "first_native":first_evidence,"second_native":second_evidence,
                "first_matches":first_matches,"second_matches":second_matches,
                "idempotent":idempotent,"first_diagnostics_match":first_diagnostics,
                "stable_diagnostics_match":stable_diagnostics,
                "computed_byte_change_matches_original_check":byte_change_matches,
                "source_unchanged_by_native":source_unchanged,
                "first_candidate_unchanged_by_native":first_candidate_unchanged,
                "passed":passed,
            }));
        }
    }
    // Retain every cell before the aggregate assertion, including failures.
    if let Some(destination) = std::env::var_os("MAMBA_QUALITY_ISORT_INSERTION_MARKER_EVIDENCE") {
        fs::write(
            destination,
            serde_json::to_vec_pretty(&json!({
                "schema":"mamba.quality.isort-insertion-marker-comparison.v1",
                "reference_sha256":REFERENCE_SHA256,"cases":results,"failed":failed,
                "native_process_exits_measured":false,"whole_suite_accepted":false,
            }))
            .unwrap(),
        )
        .unwrap();
    }
    assert_eq!(results.len(), 18);
    assert_eq!(
        failed, 0,
        "Pinned isort insertion marker comparisons failed"
    );
}
