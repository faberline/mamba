//! Finite SetComp rows from Root-qualified fresh original JSON and text captures.
use crate::domain::{model::CompletionStatus, ports::ProjectStore, source::PythonVersion};
use crate::infrastructure::filesystem::Filesystem;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs;

#[test]
fn pinned_original_set_comprehension_diagnostics() {
    let oracle_bytes = include_bytes!("set_comprehensions_oracles.json");
    let source_bytes = include_bytes!("set_comprehensions_sources.json");
    assert_eq!(
        format!("{:x}", Sha256::digest(oracle_bytes)),
        "e58e27597d37d1e79ed258f8f5841b8173581165412386312ff309c3eff43e2d"
    );
    assert_eq!(
        format!("{:x}", Sha256::digest(source_bytes)),
        "c4c0782f88b18d61804a686afa2ad2139ee1d17757fe92706ac71e9e9bc49bbb"
    );
    let oracles: Value = serde_json::from_slice(oracle_bytes).unwrap();
    let source_definition: Value = serde_json::from_slice(source_bytes).unwrap();
    let oracles = oracles.as_array().unwrap();
    assert_eq!(oracles.len(), 12);
    let mut results = Vec::new();
    let mut failures = 0;
    for oracle in oracles {
        let case = oracle["case"].as_str().unwrap();
        let source = source_definition["inputs"]
            .as_array()
            .unwrap()
            .iter()
            .find(|input| input["name"].as_str() == Some(case))
            .unwrap()["source"]
            .as_str()
            .unwrap();
        assert_eq!(
            format!("{:x}", Sha256::digest(source.as_bytes())),
            oracle["source_sha256"].as_str().unwrap()
        );
        let root = tempfile::tempdir().unwrap();
        let root_path = fs::canonicalize(root.path()).unwrap();
        let path = root_path.join("candidate.py");
        fs::write(&path, source.as_bytes()).unwrap();
        let before = fs::read(&path).unwrap();
        let mut workspace = Filesystem.load(std::slice::from_ref(&path), true);
        workspace.projects[0].python_version = match oracle["target"].as_str().unwrap() {
            "3.12" => PythonVersion::Py312,
            "3.13" => PythonVersion::Py313,
            "3.14" => PythonVersion::Py314,
            other => panic!("unexpected frozen target {other}"),
        };
        let report = super::super::check(&workspace);
        // This is the same public-field view used by existing callable originals.
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
        let text_exit_matches =
            oracle["text_original_exit"].as_i64().unwrap() == i64::from(diagnostic_status);
        let contract_preserved = report.completion == CompletionStatus::Incomplete
            && report.reference_exit_code.is_none()
            && report.checked_files == 1;
        let after = fs::read(&path).unwrap();
        let source_preserved = after == before && before == source.as_bytes();
        let matched =
            diagnostics_equal && text_exit_matches && contract_preserved && source_preserved;
        failures += usize::from(!matched);
        results.push(json!({
            "target": oracle["target"], "case": case,
            "expected": oracle["diagnostics"], "actual": actual,
            "diagnostics_equal": diagnostics_equal,
            "json_original_exit": oracle["json_original_exit"],
            "text_original_exit": oracle["text_original_exit"],
            "computed_diagnostic_status": diagnostic_status,
            "text_exit_matches": text_exit_matches,
            "completion": report.completion,
            "reference_exit": report.reference_exit_code,
            "checked_files": report.checked_files,
            "source_preserved": source_preserved,
            "raw_report": serde_json::to_value(&report).unwrap(),
            "remaining": report.remaining,
            "source_path": path,
            "source_before_hex": before.iter().map(|byte| format!("{byte:02x}")).collect::<String>(),
            "source_after_hex": after.iter().map(|byte| format!("{byte:02x}")).collect::<String>(),
            "matched": matched,
        }));
    }
    if let Some(destination) = std::env::var_os("MAMBA_QUALITY_MYPY_SETCOMP_NATIVE_EVIDENCE") {
        fs::write(
            destination,
            serde_json::to_vec_pretty(&json!({
                "schema": "mamba.quality.mypy-setcomp-original-comparison.v1",
                "oracle_fixture_sha256": format!("{:x}", Sha256::digest(oracle_bytes)),
                "source_definition_sha256": format!("{:x}", Sha256::digest(source_bytes)),
                "cells": results, "failed": failures,
                "native_reference_complete": false, "whole_acceptance": false,
            }))
            .unwrap(),
        )
        .unwrap();
    }
    assert_eq!(failures, 0, "Frozen SetComp diagnostics did not match");
}
