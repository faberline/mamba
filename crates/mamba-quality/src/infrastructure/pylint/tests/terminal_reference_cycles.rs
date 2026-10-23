//! Frozen original Pylint diagnostics for terminal reference cycles and aliases.
use crate::domain::{model::CompletionStatus, ports::ProjectStore, source::PythonVersion};
use crate::infrastructure::filesystem::Filesystem;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs;

const CORPUS: &str = include_str!("terminal_reference_cycles.json");

fn source_bytes(hex: &str) -> Vec<u8> {
    hex.as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let digit = |value: u8| (value as char).to_digit(16).expect("hex digit") as u8;
            (digit(pair[0]) << 4) | digit(pair[1])
        })
        .collect()
}

fn normalize_case_root(diagnostics: &mut Value, case_root: &str) {
    for diagnostic in diagnostics.as_array_mut().unwrap() {
        let message = diagnostic["message"].as_str().unwrap();
        diagnostic["message"] = json!(message.replace(case_root, "<case-root>"));
    }
}

#[test]
fn original_terminal_reference_cycle_diagnostics() {
    let cases: Vec<Value> = serde_json::from_str(CORPUS).unwrap();
    assert_eq!(cases.len(), 6);
    let mut records = Vec::new();
    let mut failures = 0;
    for case in cases {
        let source = source_bytes(case["source_hex"].as_str().unwrap());
        assert_eq!(
            format!("{:x}", Sha256::digest(&source)),
            case["source_sha256"].as_str().unwrap()
        );
        for (target, version) in [
            PythonVersion::Py312,
            PythonVersion::Py313,
            PythonVersion::Py314,
        ]
        .into_iter()
        .enumerate()
        {
            let root = tempfile::tempdir().unwrap();
            let root_path = fs::canonicalize(root.path()).unwrap();
            fs::write(root_path.join("main.py"), &source).unwrap();
            fs::write(
                root_path.join("pyproject.toml"),
                format!("[project]\nrequires-python = \">={}\"\n", version.name()),
            )
            .unwrap();
            let workspace = Filesystem.load(std::slice::from_ref(&root_path), true);
            assert_eq!(workspace.projects[0].python_version, version);
            let report = super::super::check(&workspace);
            let mut actual = Value::Array(
                report
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
                    .collect(),
            );
            let mut expected = case["expected"][target].clone();
            normalize_case_root(&mut actual, root_path.to_str().unwrap());
            normalize_case_root(
                &mut expected,
                case["reference_case_roots"][target].as_str().unwrap(),
            );
            let diagnostic_exit = report.diagnostics.iter().fold(0, |exit, diagnostic| {
                exit | match diagnostic.severity.as_str() {
                    "fatal" => 1,
                    "error" => 2,
                    "warning" => 4,
                    "refactor" => 8,
                    "convention" => 16,
                    "info" => 0,
                    severity => panic!("Unknown severity: {severity}"),
                }
            });
            let source_preserved = fs::read(root_path.join("main.py")).unwrap() == source;
            let equal = actual == expected
                && diagnostic_exit == case["reference_exits"][target].as_i64().unwrap()
                && report.completion == CompletionStatus::Incomplete
                && report.reference_exit_code.is_none()
                && report.checked_files == 1
                && source_preserved;
            failures += usize::from(!equal);
            records.push(json!({
                "name": case["name"], "python": version.name(),
                "expected": expected, "actual": actual, "equal": equal,
                "diagnostic_exit": diagnostic_exit,
                "reference_exit": case["reference_exits"][target],
                "completion": format!("{:?}", report.completion),
                "reference_exit_code": report.reference_exit_code,
                "checked_files": report.checked_files,
                "source_preserved": source_preserved,
                "source_sha256": case["source_sha256"],
            }));
        }
    }
    if let Some(destination) = std::env::var_os("MAMBA_QUALITY_PYLINT_TERMINAL_EVIDENCE") {
        fs::write(
            destination,
            serde_json::to_vec_pretty(&json!({
                "schema": "mamba.quality.pylint-terminal-reference-comparison.v1",
                "corpus_sha256": format!("{:x}", Sha256::digest(CORPUS.as_bytes())),
                "comparisons": records.len(), "failed": failures,
                "records": records, "whole_tool_accepted": false,
            }))
            .unwrap(),
        )
        .unwrap();
    }
    assert_eq!(records.len(), 18);
    assert_eq!(failures, 0, "Frozen Pylint terminal diagnostics differ");
}
