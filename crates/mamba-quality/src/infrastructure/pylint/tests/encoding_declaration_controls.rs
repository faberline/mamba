//! Locked raw-byte source cases retain original Pylint encoding diagnostics.
use crate::domain::{model::CompletionStatus, ports::ProjectStore, source::PythonVersion};
use crate::infrastructure::filesystem::Filesystem;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs;

fn bytes_from_hex(source: &str) -> Vec<u8> {
    assert_eq!(source.len() % 2, 0);
    source
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
}

fn normalize_case_root(diagnostics: &mut Value, root: &str) {
    let prefix = format!("{root}/");
    for diagnostic in diagnostics.as_array_mut().unwrap() {
        let message = diagnostic["message"].as_str().unwrap();
        if message.contains(&prefix) {
            diagnostic["message"] = Value::String(message.replace(&prefix, "<case>/"));
        }
    }
}

#[test]
fn pinned_original_encoding_declaration_controls() {
    let corpus = include_str!("encoding_declaration_controls.json");
    let cases: Vec<Value> = serde_json::from_str(corpus).unwrap();
    assert_eq!(cases.len(), 6);
    let mut results = Vec::new();
    let mut failures = 0;
    for case in cases {
        let root = tempfile::tempdir().unwrap();
        let root_path = fs::canonicalize(root.path()).unwrap();
        fs::write(
            root_path.join("pyproject.toml"),
            "[project]\nrequires-python = '>=3.12'\n",
        )
        .unwrap();
        let original_bytes = bytes_from_hex(case["source_hex"].as_str().unwrap());
        assert_eq!(
            format!("{:x}", Sha256::digest(&original_bytes)),
            case["source_sha256"].as_str().unwrap()
        );
        let path = root_path.join("main.py");
        fs::write(&path, &original_bytes).unwrap();
        let mut workspace = Filesystem.load(std::slice::from_ref(&path), true);
        assert_eq!(workspace.sources.len(), 1);
        assert_eq!(workspace.sources[0].bytes, original_bytes);
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
            let actual_raw: Vec<_> = report.diagnostics.iter().map(|diagnostic| json!({
                "rule": diagnostic.rule, "message": diagnostic.message, "hint": diagnostic.hint,
                "path": diagnostic.path.strip_prefix(&root_path).unwrap(),
                "severity": diagnostic.severity,
                "location": {"line": diagnostic.location.line, "column": diagnostic.location.column,
                    "end_line": diagnostic.location.end_line, "end_column": diagnostic.location.end_column},
            })).collect();
            let diagnostic_exit = report.diagnostics.iter().fold(0, |exit, diagnostic| {
                exit | match diagnostic.severity.as_str() {
                    "fatal" => 1,
                    "error" => 2,
                    "warning" => 4,
                    "refactor" => 8,
                    "convention" => 16,
                    "info" => 0,
                    severity => panic!("Unknown original Pylint severity: {severity}"),
                }
            });
            let actual_raw = Value::Array(actual_raw);
            let expected_raw = case["expected"][target].clone();
            let mut actual = actual_raw.clone();
            let mut expected = expected_raw.clone();
            normalize_case_root(&mut actual, root_path.to_str().unwrap());
            normalize_case_root(
                &mut expected,
                case["reference_case_roots"][target].as_str().unwrap(),
            );
            let source_bytes_unchanged = fs::read(&path).unwrap() == original_bytes;
            let equal = actual == expected
                && case["reference_exits"][target].as_i64() == Some(i64::from(diagnostic_exit))
                && report.checked_files == 1
                && report.completion == CompletionStatus::Incomplete
                && report.reference_exit_code.is_none()
                && source_bytes_unchanged;
            failures += usize::from(!equal);
            results.push(json!({
                "name": case["name"], "python": version.name(), "source_hex": case["source_hex"],
                "source_sha256": case["source_sha256"], "expected_raw": expected_raw,
                "actual_raw": actual_raw, "expected": expected, "actual": actual, "equal": equal,
                "reference_exit": case["reference_exits"][target], "diagnostic_exit": diagnostic_exit,
                "checked_files": report.checked_files, "actual_completion": report.completion,
                "actual_reference_exit": report.reference_exit_code,
                "source_bytes_unchanged": source_bytes_unchanged,
                "decoding_issue": workspace.sources[0].decoding_issue.as_ref().map(|issue| issue.message.as_str()),
            }));
        }
    }
    if let Some(destination) =
        std::env::var_os("MAMBA_QUALITY_PYLINT_ENCODING_DECLARATION_CONTROLS_EVIDENCE")
    {
        fs::write(
            destination,
            serde_json::to_vec_pretty(&json!({
                "schema": "mamba.quality.pylint-encoding-declaration-controls-comparison.v1",
                "corpus_sha256": format!("{:x}", Sha256::digest(corpus.as_bytes())),
                "cases": results, "failed": failures, "whole_suite_accepted": false,
            }))
            .unwrap(),
        )
        .unwrap();
    }
    assert_eq!(
        failures, 0,
        "Pinned Pylint encoding declaration controls differ"
    );
}

#[test]
fn protected_original_utf32_declaration() {
    let case = super::catalog::GROUPS
        .iter()
        .flat_map(|group| group.iter())
        .flat_map(|chunk| serde_json::from_str::<Vec<Value>>(chunk).unwrap())
        .find(|case| case["name"] == "unicode_controls1_encoding_7")
        .expect("frozen original UTF-32 declaration case");
    let root = tempfile::tempdir().unwrap();
    let root_path = fs::canonicalize(root.path()).unwrap();
    fs::write(
        root_path.join("pyproject.toml"),
        "[project]\nrequires-python = '>=3.12'\n",
    )
    .unwrap();
    let source = case["files"]["main.py"].as_str().unwrap();
    assert_eq!(
        format!("{:x}", Sha256::digest(source.as_bytes())),
        case["source_sha256"]["main.py"].as_str().unwrap()
    );
    let path = root_path.join("main.py");
    fs::write(&path, source).unwrap();
    let mut workspace = Filesystem.load(std::slice::from_ref(&path), true);
    let mut results = Vec::new();
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
        let actual_raw: Vec<_> = report.diagnostics.iter().map(|diagnostic| json!({
            "rule":diagnostic.rule,"message":diagnostic.message,"hint":diagnostic.hint,
            "path":diagnostic.path.strip_prefix(&root_path).unwrap(),
            "severity":diagnostic.severity,
            "location":{"line":diagnostic.location.line,"column":diagnostic.location.column,
                "end_line":diagnostic.location.end_line,"end_column":diagnostic.location.end_column}
        })).collect();
        let mut actual = Value::Array(actual_raw);
        let mut expected = case["expected"][target].clone();
        super::path_prefix::normalize(&mut actual);
        super::path_prefix::normalize(&mut expected);
        let exit = report.diagnostics.iter().fold(0, |bits, diagnostic| {
            bits | match diagnostic.severity.as_str() {
                "fatal" => 1,
                "error" => 2,
                "warning" => 4,
                "refactor" => 8,
                "convention" => 16,
                "info" => 0,
                severity => panic!("Unknown original Pylint severity: {severity}"),
            }
        });
        let equal = actual == expected
            && case["reference_exits"][target].as_i64() == Some(exit)
            && report.checked_files == 1
            && report.completion == CompletionStatus::Incomplete
            && report.reference_exit_code.is_none()
            && fs::read(&path).unwrap() == source.as_bytes();
        results.push(
            json!({"name":case["name"],"python":version.name(),"equal":equal,
            "expected":expected,"actual":actual,"reference_exit":case["reference_exits"][target],
            "diagnostic_exit":exit,"completion":report.completion,
            "reference_exit_code":report.reference_exit_code,"checked_files":report.checked_files}),
        );
    }
    if let Some(destination) = std::env::var_os("MAMBA_QUALITY_PYLINT_UTF32_CONTROL_EVIDENCE") {
        fs::write(
            destination,
            serde_json::to_vec_pretty(&json!({
                "schema":"mamba.quality.pylint-original-utf32-control.v1",
                "failed":results.iter().filter(|row| row["equal"] == false).count(),
                "cases":results,"whole_suite_accepted":false,
            }))
            .unwrap(),
        )
        .unwrap();
    }
    assert!(results.iter().all(|row| row["equal"] == true));
}
