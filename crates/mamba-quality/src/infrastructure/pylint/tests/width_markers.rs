//! Frozen finite controls for Pylint inline-marker line-length behavior.
//!
//! These all-empty originals are negative/marker controls. They do not prove
//! positive C0301 width coverage or complete Pylint execution.
use crate::domain::{model::CompletionStatus, ports::ProjectStore, source::PythonVersion};
use crate::infrastructure::filesystem::Filesystem;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs;

const ORIGINAL_FIXTURE_SHA256: &str =
    "c92ba94da2c51103d112eadbae4cced6332df3dd267bcfd53766c35139f18589";
const INPUT_FREEZE_SHA256: &str =
    "784c45c829e0b2b4e82cf2c3bac38bf93463f4d0abb9881e4360a4ab77a0e991";
const PROPOSAL_FIXTURE_SHA256: &str =
    "f4442bc2ec8aae410e1c2c54676cb0ca6eb99b879bb5559f8ff1b57f622a07e6";

#[test]
fn pinned_original_inline_marker_controls() {
    let corpus = include_str!("width_markers.json");
    assert_eq!(
        format!("{:x}", Sha256::digest(corpus.as_bytes())),
        PROPOSAL_FIXTURE_SHA256
    );
    let cases: Value = serde_json::from_str(corpus).unwrap();
    assert_eq!(cases["source_fixture_sha256"], ORIGINAL_FIXTURE_SHA256);
    assert_eq!(cases["input_freeze_sha256"], INPUT_FREEZE_SHA256);
    assert_eq!(cases["cells"].as_array().unwrap().len(), 24);

    let mut results = Vec::new();
    let mut failures = 0usize;
    for case in cases["cells"].as_array().unwrap() {
        let source_bytes = decode_hex(case["source_hex"].as_str().unwrap());
        let source_hash = format!("{:x}", Sha256::digest(&source_bytes));
        let source_hash_matches = source_hash == case["source_sha256"].as_str().unwrap();
        let root = tempfile::tempdir().unwrap();
        let root_path = fs::canonicalize(root.path()).unwrap();
        let path = root_path.join("main.py");
        fs::write(&path, &source_bytes).unwrap();
        let mut workspace = Filesystem.load(std::slice::from_ref(&path), true);
        let version = match case["target"].as_str().unwrap() {
            "py312" => PythonVersion::Py312,
            "py313" => PythonVersion::Py313,
            "py314" => PythonVersion::Py314,
            target => panic!("Unknown frozen Python target: {target}"),
        };
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
        let expected = &case["repeat_1"]["diagnostics"];
        let repeat_pair_is_stable = case["stable"] == true
            && case["repeat_1"]["diagnostics"] == case["repeat_2"]["diagnostics"]
            && case["repeat_1"]["stdout_sha256"] == case["repeat_2"]["stdout_sha256"]
            && case["repeat_1"]["stderr_sha256"] == case["repeat_2"]["stderr_sha256"]
            && case["repeat_1"]["exit_code"] == case["repeat_2"]["exit_code"];
        let source_after = fs::read(&path).unwrap();
        let source_unchanged = source_after == source_bytes;
        let completion_is_incomplete = report.completion == CompletionStatus::Incomplete;
        let reference_exit_is_null = report.reference_exit_code.is_none();
        let checked_one_file = report.checked_files == 1;
        let equal = source_hash_matches
            && repeat_pair_is_stable
            && actual == *expected
            && source_unchanged
            && completion_is_incomplete
            && reference_exit_is_null
            && checked_one_file;
        failures += usize::from(!equal);
        results.push(json!({
            "case": case["case"],
            "target": case["target"],
            "source_sha256": source_hash,
            "source_hash_matches_frozen": source_hash_matches,
            "expected_diagnostics": expected,
            "actual_diagnostics": actual,
            "original_cli_exit_codes_provenance_only": [
                case["repeat_1"]["exit_code"], case["repeat_2"]["exit_code"]
            ],
            "original_stdout_sha256": [
                case["repeat_1"]["stdout_sha256"], case["repeat_2"]["stdout_sha256"]
            ],
            "original_stderr_sha256": [
                case["repeat_1"]["stderr_sha256"], case["repeat_2"]["stderr_sha256"]
            ],
            "original_repeat_pair_stable": repeat_pair_is_stable,
            "actual_completion": report.completion,
            "actual_reference_exit_code": report.reference_exit_code,
            "actual_checked_files": report.checked_files,
            "source_unchanged": source_unchanged,
            "matches": equal,
        }));
    }

    assert_eq!(results.len(), 24);
    if let Some(destination) = std::env::var_os("MAMBA_QUALITY_PYLINT_WIDTH_MARKERS_EVIDENCE") {
        fs::write(destination, serde_json::to_vec_pretty(&json!({
            "schema": "mamba.quality.pylint-width-marker-comparison-proposal.v1",
            "proposal_fixture_sha256": format!("{:x}", Sha256::digest(corpus.as_bytes())),
            "source_fixture_sha256": ORIGINAL_FIXTURE_SHA256,
            "input_freeze_sha256": INPUT_FREEZE_SHA256,
            "cases": results,
            "failed": failures,
            "whole_suite_accepted": false,
            "scope": "Finite negative and marker controls only; no positive C0301 width-completeness claim.",
        })).unwrap()).unwrap();
    }
    assert_eq!(failures, 0, "Frozen Pylint inline-marker controls differ");
}

fn decode_hex(value: &str) -> Vec<u8> {
    assert_eq!(value.len() % 2, 0, "Frozen source hex has odd length");
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let digit = |byte: u8| match byte {
                b'0'..=b'9' => byte - b'0',
                b'a'..=b'f' => byte - b'a' + 10,
                b'A'..=b'F' => byte - b'A' + 10,
                _ => panic!("Invalid hex digit in frozen source"),
            };
            (digit(pair[0]) << 4) | digit(pair[1])
        })
        .collect()
}
