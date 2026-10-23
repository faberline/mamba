//! Qualified original dict.fromkeys diagnostics retain all nine JSON fields.
use crate::domain::{model::CompletionStatus, ports::ProjectStore, source::PythonVersion};
use crate::infrastructure::filesystem::Filesystem;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs;

// Source: /private/tmp/mamba-quality-next-mypy-gap-analysis-20261016-1/fixtures/fromkeys.py
const SOURCE_SHA256: &str = "cdc09aea5fa55175d130554f23ebac4c6ed700de516211ba3a519dbcd4e42518";
const SOURCE: &str = r###"def deduplicate(keys: list[str]) -> list[str]:
    return list(dict.fromkeys(keys))


def none_values(keys: list[str]) -> dict[str, None]:
    return dict.fromkeys(keys)


def int_values(keys: list[str]) -> dict[str, int]:
    return dict.fromkeys(keys, 1)


def reject_key_type(keys: list[str]) -> dict[int, None]:
    return dict.fromkeys(keys)


def reject_value_type(keys: list[str]) -> dict[str, str]:
    return dict.fromkeys(keys, 1)


def preserve_constructor() -> dict[str, int]:
    return dict(answer=42)


class Local:
    def fromkeys(self, keys: list[str]) -> str:
        return ",".join(keys)


def local_shadow(dict: Local) -> str:
    return dict.fromkeys(["a"])
"###;

// Exact qualified record. Its raw columns and all original fields stay unchanged.
// Source: /private/tmp/mamba-quality-dict-fromkeys-originals-qualified-20261016-1/qualified-cases/fromkeys.json
// Controller proof: /private/tmp/mamba-quality-dict-fromkeys-qualification-main-execution-20261016-1/process.json
// Each target was captured twice with Mypy 2.4.0, Pydantic 2.13.5, strict flags,
// the fixed plugin config, and platform darwin. This test runs only native Rust.
const ORIGINAL_SHA256: &str = "f9957516f593a18ec5af02813bb9f960a7729a8b25c33de7f24f1fcf470f5e1a";
const ORIGINAL: &str = r###"{
  "case": "fromkeys",
  "diagnostics_are_native_expectations": false,
  "original_parity_accepted": false,
  "performance_acceptance": false,
  "schema": "mamba.dict-fromkeys.original-json-qualified-case.v1",
  "source_config_path": "/private/tmp/mamba-quality-dict-fromkeys-originals-proposal-20261016-2/fixed-profile-mypy.ini",
  "source_config_sha256": "3a30dac6faba76ed57f03eaf744188596b45ed86ba05d555b57788ddc67459c5",
  "source_path": "/private/tmp/mamba-quality-next-mypy-gap-analysis-20261016-1/fixtures/fromkeys.py",
  "source_sha256": "cdc09aea5fa55175d130554f23ebac4c6ed700de516211ba3a519dbcd4e42518",
  "status": "capture_qualified_for_future_comparison_only",
  "targets": {
    "3.12": {
      "actual_cli_exit_code": 1,
      "diagnostics": [
        {
          "code": "arg-type",
          "column": 25,
          "end_column": 29,
          "end_line": 14,
          "file": "fromkeys.py",
          "hint": null,
          "line": 14,
          "message": "Argument 1 to \"fromkeys\" of \"dict\" has incompatible type \"list[str]\"; expected \"Iterable[int]\"",
          "severity": "error"
        },
        {
          "code": "arg-type",
          "column": 31,
          "end_column": 32,
          "end_line": 18,
          "file": "fromkeys.py",
          "hint": null,
          "line": 18,
          "message": "Argument 2 to \"fromkeys\" of \"dict\" has incompatible type \"int\"; expected \"str\"",
          "severity": "error"
        }
      ],
      "guard_observed_runtime_python_version": [
        3,
        12,
        11
      ],
      "historical_runtime_version": "3.12.11",
      "mypy_version": "2.4.0",
      "platform": "darwin",
      "presentation_mode": "json",
      "pydantic_version": "2.13.5",
      "raw_captures": "/private/tmp/mamba-quality-dict-fromkeys-originals-qualified-20261016-1/raw-captures/3.12",
      "repeat_count": 2,
      "repeat_raw_stderr_sha256": [
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
      ],
      "repeat_raw_stdout_sha256": [
        "ae059bd37125bfc030bf14f2fd05c65291b5151187dadfdf58df8323ae6ebba3",
        "ae059bd37125bfc030bf14f2fd05c65291b5151187dadfdf58df8323ae6ebba3"
      ]
    },
    "3.13": {
      "actual_cli_exit_code": 1,
      "diagnostics": [
        {
          "code": "arg-type",
          "column": 25,
          "end_column": 29,
          "end_line": 14,
          "file": "fromkeys.py",
          "hint": null,
          "line": 14,
          "message": "Argument 1 to \"fromkeys\" of \"dict\" has incompatible type \"list[str]\"; expected \"Iterable[int]\"",
          "severity": "error"
        },
        {
          "code": "arg-type",
          "column": 31,
          "end_column": 32,
          "end_line": 18,
          "file": "fromkeys.py",
          "hint": null,
          "line": 18,
          "message": "Argument 2 to \"fromkeys\" of \"dict\" has incompatible type \"int\"; expected \"str\"",
          "severity": "error"
        }
      ],
      "guard_observed_runtime_python_version": [
        3,
        13,
        5
      ],
      "historical_runtime_version": "3.13.5",
      "mypy_version": "2.4.0",
      "platform": "darwin",
      "presentation_mode": "json",
      "pydantic_version": "2.13.5",
      "raw_captures": "/private/tmp/mamba-quality-dict-fromkeys-originals-qualified-20261016-1/raw-captures/3.13",
      "repeat_count": 2,
      "repeat_raw_stderr_sha256": [
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
      ],
      "repeat_raw_stdout_sha256": [
        "ae059bd37125bfc030bf14f2fd05c65291b5151187dadfdf58df8323ae6ebba3",
        "ae059bd37125bfc030bf14f2fd05c65291b5151187dadfdf58df8323ae6ebba3"
      ]
    },
    "3.14": {
      "actual_cli_exit_code": 1,
      "diagnostics": [
        {
          "code": "arg-type",
          "column": 25,
          "end_column": 29,
          "end_line": 14,
          "file": "fromkeys.py",
          "hint": null,
          "line": 14,
          "message": "Argument 1 to \"fromkeys\" of \"dict\" has incompatible type \"list[str]\"; expected \"Iterable[int]\"",
          "severity": "error"
        },
        {
          "code": "arg-type",
          "column": 31,
          "end_column": 32,
          "end_line": 18,
          "file": "fromkeys.py",
          "hint": null,
          "line": 18,
          "message": "Argument 2 to \"fromkeys\" of \"dict\" has incompatible type \"int\"; expected \"str\"",
          "severity": "error"
        }
      ],
      "guard_observed_runtime_python_version": [
        3,
        14,
        0
      ],
      "historical_runtime_version": "3.14.0",
      "mypy_version": "2.4.0",
      "platform": "darwin",
      "presentation_mode": "json",
      "pydantic_version": "2.13.5",
      "raw_captures": "/private/tmp/mamba-quality-dict-fromkeys-originals-qualified-20261016-1/raw-captures/3.14",
      "repeat_count": 2,
      "repeat_raw_stderr_sha256": [
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
      ],
      "repeat_raw_stdout_sha256": [
        "ae059bd37125bfc030bf14f2fd05c65291b5151187dadfdf58df8323ae6ebba3",
        "ae059bd37125bfc030bf14f2fd05c65291b5151187dadfdf58df8323ae6ebba3"
      ]
    }
  },
  "whole_plan_accepted": false,
  "whole_som_accepted": false
}
"###;

#[test]
fn qualified_original_dict_fromkeys() {
    assert_eq!(
        format!("{:x}", Sha256::digest(SOURCE.as_bytes())),
        SOURCE_SHA256
    );
    assert_eq!(
        format!("{:x}", Sha256::digest(ORIGINAL.as_bytes())),
        ORIGINAL_SHA256
    );
    let original: Value = serde_json::from_str(ORIGINAL).unwrap();
    assert_eq!(original["source_sha256"], SOURCE_SHA256);
    assert_eq!(original["targets"].as_object().unwrap().len(), 3);
    let mut results = Vec::new();
    let mut failures = 0;
    for (target, version) in [
        ("3.12", PythonVersion::Py312),
        ("3.13", PythonVersion::Py313),
        ("3.14", PythonVersion::Py314),
    ] {
        let cell = &original["targets"][target];
        assert_eq!(cell["mypy_version"], "2.4.0");
        assert_eq!(cell["pydantic_version"], "2.13.5");
        assert_eq!(cell["platform"], "darwin");
        assert_eq!(cell["presentation_mode"], "json");
        assert_eq!(cell["repeat_count"], 2);
        assert_eq!(cell["actual_cli_exit_code"], 1);
        let mut expected = cell["diagnostics"].clone();
        for diagnostic in expected.as_array_mut().unwrap() {
            assert_eq!(diagnostic.as_object().unwrap().len(), 9);
            assert_eq!(diagnostic["hint"], Value::Null);
            // This is the existing tools/python_quality/mypy_corpus.py mapping.
            // Only the start column changes. End columns keep their raw values.
            diagnostic["column"] = json!(diagnostic["column"].as_u64().unwrap() + 1);
        }
        let root = tempfile::tempdir().unwrap();
        let root_path = fs::canonicalize(root.path()).unwrap();
        let path = root_path.join("fromkeys.py");
        fs::write(&path, SOURCE).unwrap();
        let mut workspace = Filesystem.load(std::slice::from_ref(&path), true);
        assert_eq!(workspace.projects.len(), 1);
        workspace.projects[0].python_version = version;
        let report = super::super::check(&workspace);
        let actual: Vec<_> = report
            .diagnostics
            .iter()
            .map(|diagnostic| {
                json!({
                    "file": diagnostic.path.strip_prefix(&root_path).unwrap(),
                    "line": diagnostic.location.line,
                    "column": diagnostic.location.column,
                    "end_line": diagnostic.location.end_line,
                    "end_column": diagnostic.location.end_column,
                    "severity": diagnostic.severity,
                    "code": serde_json::to_value(diagnostic).unwrap()["rule"],
                    "message": diagnostic.message,
                    "hint": diagnostic.hint,
                })
            })
            .collect();
        let diagnostic_status = i32::from(!actual.is_empty());
        let actual = Value::Array(actual);
        let source_preserved = fs::read(&path).unwrap() == SOURCE.as_bytes();
        let contract_matches = report.completion == CompletionStatus::Incomplete
            && report.reference_exit_code.is_none()
            && report.checked_files == 1;
        let matched = actual == expected
            && cell["actual_cli_exit_code"].as_i64() == Some(i64::from(diagnostic_status))
            && source_preserved
            && contract_matches;
        failures += usize::from(!matched);
        results.push(json!({
            "target": target,
            "source_sha256": SOURCE_SHA256,
            "source_preserved": source_preserved,
            "original_json_diagnostics": cell["diagnostics"],
            "expected_native_diagnostics": expected,
            "actual_native_diagnostics": actual,
            "original_actual_cli_exit_code": cell["actual_cli_exit_code"],
            "computed_diagnostic_status": diagnostic_status,
            "actual_completion": report.completion,
            "actual_reference_exit_code": report.reference_exit_code,
            "actual_checked_files": report.checked_files,
            "contract_matches": contract_matches,
            "matched": matched,
        }));
    }
    assert_eq!(results.len(), 3);
    if let Some(destination) = std::env::var_os("MAMBA_QUALITY_MYPY_DICT_FROMKEYS_EVIDENCE") {
        let output = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(destination)
            .unwrap();
        serde_json::to_writer_pretty(
            output,
            &json!({
                "schema": "mamba.quality.mypy.dict-fromkeys-comparison.v1",
                "source_sha256": SOURCE_SHA256,
                "original_sha256": ORIGINAL_SHA256,
                "coordinate_policy": "mypy_corpus.py start column +1 only",
                "cells": results,
                "failed": failures,
                "native_reference_complete": false,
                "whole_plan_accepted": false,
                "performance_acceptance": false,
            }),
        )
        .unwrap();
    }
    assert_eq!(
        failures, 0,
        "Qualified dict.fromkeys diagnostics did not match"
    );
}

#[test]
fn constructor_lookup_keeps_unknown_and_unbound_members() {
    use crate::infrastructure::mypy::types::Context;
    use mamba_language::application::quality_syntax::PythonTarget;

    for target in [
        PythonTarget::Py312,
        PythonTarget::Py313,
        PythonTarget::Py314,
    ] {
        let context = Context::default();
        let constructor = context.collection_constructor("dict", target).unwrap();
        let ty = context.callable_type(constructor);
        assert!(context
            .builtin_constructor_member(ty, "missing_mamba_fromkeys_guard", target)
            .is_none());
        // An instance method must not become a bound class method.
        if let Some(keys) = context.builtin_constructor_member(ty, "keys", target) {
            assert!(keys.bound_receiver.is_none());
            assert!(keys.is_unbound_builtin_member());
        }
        let get = context
            .builtin_constructor_member(ty, "get", target)
            .unwrap();
        assert!(get.bound_receiver.is_none());
        assert!(get.is_unbound_builtin_member());
    }
}
