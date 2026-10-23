//! Fixed tuple assignment source and Root-qualified original diagnostics.
use crate::domain::{model::CompletionStatus, ports::ProjectStore, source::PythonVersion};
use crate::infrastructure::filesystem::Filesystem;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{fs, io::Write};

#[test]
fn pinned_original_tuple_binding_diagnostics() {
    let source = include_bytes!("tuple_binding.py");
    let oracle_bytes = include_bytes!("tuple_binding_oracles.json");
    let source_hash = format!("{:x}", Sha256::digest(source));
    let oracle_hash = format!("{:x}", Sha256::digest(oracle_bytes));
    assert_eq!(source.len(), 76);
    assert_eq!(
        source_hash,
        "c73aeffd749d76ce7c6914c8a4f870f23410c7644a7c13e0cd8a1c8cffeb8d6f"
    );
    assert_eq!(
        oracle_hash,
        "b36ce72cc348691eae849452d62f2dca70dff5023070a4fa6438db5d802c6585"
    );
    let oracles: Vec<Value> = serde_json::from_slice(oracle_bytes).unwrap();
    assert_eq!(oracles.len(), 3);
    let mut rows = Vec::new();
    let mut failures = 0;
    for (oracle, version) in oracles.iter().zip([
        PythonVersion::Py312,
        PythonVersion::Py313,
        PythonVersion::Py314,
    ]) {
        assert_eq!(oracle["target"], version.name());
        assert_eq!(oracle["source_sha256"], source_hash);
        assert_eq!(oracle["json_original_exit"], 0);
        assert_eq!(oracle["text_original_exit"], 0);
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
        let diagnostics_equal = actual == oracle["diagnostics"];
        let diagnostic_status = i32::from(
            report
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.severity == "error"),
        );
        let original_exits_match = oracle["json_original_exit"].as_i64()
            == Some(i64::from(diagnostic_status))
            && oracle["text_original_exit"].as_i64() == Some(i64::from(diagnostic_status));
        let contract_preserved = report.completion == CompletionStatus::Incomplete
            && report.reference_exit_code.is_none()
            && report.checked_files == 1;
        let after = fs::read(&path).unwrap();
        let source_preserved = before == source.as_slice() && before == after;
        let matched =
            diagnostics_equal && original_exits_match && contract_preserved && source_preserved;
        failures += usize::from(!matched);
        rows.push(json!({
            "case": oracle["case"], "target": version.name(),
            "source_sha256": source_hash, "source_path": path,
            "expected": oracle["diagnostics"], "actual": actual,
            "json_original_exit": oracle["json_original_exit"],
            "text_original_exit": oracle["text_original_exit"],
            "raw_json_sha256": oracle["raw_json_sha256"],
            "raw_text_sha256": oracle["raw_text_sha256"],
            "computed_diagnostic_status": diagnostic_status,
            "original_exits_match": original_exits_match,
            "diagnostics_equal": diagnostics_equal,
            "contract_preserved": contract_preserved,
            "source_preserved": source_preserved,
            "source_before_hex": before.iter().map(|byte| format!("{byte:02x}")).collect::<String>(),
            "source_after_hex": after.iter().map(|byte| format!("{byte:02x}")).collect::<String>(),
            "raw_report": serde_json::to_value(&report).unwrap(),
            "matched": matched,
        }));
    }
    assert_eq!(rows.len(), 3);
    if let Some(destination) = std::env::var_os("MAMBA_QUALITY_MYPY_TUPLE_BINDING_EVIDENCE") {
        let mut output = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(destination)
            .unwrap();
        output
            .write_all(
                &serde_json::to_vec_pretty(&json!({
                    "schema": "mamba.quality.mypy-tuple-binding-original-comparison.v1",
                    "oracle_fixture_sha256": oracle_hash,
                    "source_fixture_sha256": source_hash,
                    "original_fixture_utf8": std::str::from_utf8(oracle_bytes).unwrap(),
                    "cells": rows, "failed": failures,
                    "native_reference_complete": false,
                    "whole_acceptance": false,
                }))
                .unwrap(),
            )
            .unwrap();
    }
    assert_eq!(
        failures, 0,
        "Frozen tuple binding diagnostics did not match"
    );
}
