//! Compare Black AST-safety errors and their raw diff artifacts.
use super::super::*;
use crate::domain::{
    model::{CompletionStatus, QualityTool},
    ports::ProjectStore,
    source::PythonVersion,
};
use crate::infrastructure::filesystem::Filesystem;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::{fs, path::Path};

const MESSAGE_MARKER: &str = "This diff might be helpful: ";

fn message_parts(message: &str) -> Option<(&str, &Path)> {
    let (prefix, path) = message.rsplit_once(MESSAGE_MARKER)?;
    let path = Path::new(path);
    let name = path.file_name()?.to_str()?;
    let suffix = name.strip_prefix("blk_")?.strip_suffix(".log")?;
    if suffix.is_empty()
        || !suffix
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    {
        return None;
    }
    Some((prefix, path))
}

pub(super) fn message_equal(actual: &str, expected: &str) -> bool {
    message_parts(actual)
        .zip(message_parts(expected))
        .is_some_and(|(actual, expected)| actual.0 == expected.0)
}

fn canonical_log_parent_matches_current_tmp(actual: &Path) -> bool {
    let Some(actual_parent) = actual.parent().and_then(|path| path.canonicalize().ok()) else {
        return false;
    };
    let Ok(current_temp_root) = std::env::temp_dir().canonicalize() else {
        return false;
    };
    actual_parent == current_temp_root
}

fn decode_hex(text: &str) -> Option<Vec<u8>> {
    if !text.len().is_multiple_of(2) {
        return None;
    }
    text.as_bytes()
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).ok()?, 16).ok())
        .collect()
}

fn bytes_record(bytes: &[u8]) -> Value {
    json!({
        "size": bytes.len(),
        "sha256": format!("{:x}", Sha256::digest(bytes)),
        "bytes_hex": bytes.iter().map(|byte| format!("{byte:02x}")).collect::<String>()
    })
}

fn log_record(message: Option<&str>) -> Value {
    let Some((_, path)) = message.and_then(message_parts) else {
        return json!({"path":null,"bytes":null,"read_error":"missing or invalid ASTSafetyError log path"});
    };
    match fs::read(path) {
        Ok(bytes) => json!({
            "path": path.to_string_lossy(),
            "bytes": bytes_record(&bytes),
            "read_error": null
        }),
        Err(error) => json!({
            "path": path.to_string_lossy(),
            "bytes": null,
            "read_error": error.to_string()
        }),
    }
}

fn format_record(result: Result<String, crate::domain::model::QualityIssue>) -> Value {
    match result {
        Ok(output) => json!({"kind":"formatted","output":output,"error":null,"error_class":null}),
        Err(error) => json!({
            "kind":"error",
            "output":null,
            "error":error.message,
            "error_class":error.code
        }),
    }
}

fn report_log_records(report: &crate::domain::model::ToolReport) -> Vec<Value> {
    report
        .diagnostics
        .iter()
        .enumerate()
        .map(|(index, diagnostic)| {
            let mut row = Map::new();
            row.insert("diagnostic_index".into(), json!(index));
            row.insert("rule".into(), json!(diagnostic.rule));
            row.insert("artifact".into(), log_record(Some(&diagnostic.message)));
            Value::Object(row)
        })
        .collect()
}

fn source_unchanged(before: &[u8], after: &[u8], expected_sha: &str) -> bool {
    before == after && format!("{:x}", Sha256::digest(before)) == expected_sha
}

#[test]
fn canonical_log_parent_rejects_missing_path() {
    let root = tempfile::tempdir().unwrap();
    let missing = root.path().join("missing-parent").join("blk_test.log");
    assert!(!missing.parent().unwrap().exists());
    assert!(!canonical_log_parent_matches_current_tmp(&missing));
}

#[test]
fn canonical_log_parent_rejects_other_existing_root() {
    let other_root = tempfile::tempdir().unwrap();
    let other_log = other_root.path().join("blk_test.log");
    assert!(!canonical_log_parent_matches_current_tmp(&other_log));
}

#[test]
fn canonical_log_parent_accepts_current_temp_root() {
    let current_root = std::env::temp_dir().canonicalize().unwrap();
    let current_log = current_root.join("blk_test.log");
    assert!(canonical_log_parent_matches_current_tmp(&current_log));
}

#[test]
fn pinned_original_safety_errors_with_raw_evidence() {
    let corpus = include_str!("safety.json");
    let cases: Vec<Value> = serde_json::from_str(corpus).unwrap();
    let logs_text = include_str!("safety_logs.json");
    let logs: Value = serde_json::from_str(logs_text).unwrap();
    let cells = logs["cells"].as_array().unwrap();
    assert_eq!(
        logs["source_fixture_sha256"],
        format!("{:x}", Sha256::digest(corpus.as_bytes()))
    );
    assert_eq!(logs["cell_count"], 174);
    assert_eq!(cells.len(), 174);

    let root = tempfile::tempdir().unwrap();
    let root_path = fs::canonicalize(root.path()).unwrap();
    let mut rows = Vec::new();
    let mut failures = 0;
    for (index, case) in cases.into_iter().enumerate() {
        let path = root_path.join(format!("case_{index}.py"));
        let source = case["source"].as_str().unwrap();
        fs::write(&path, source).unwrap();
        let expected_source_sha = case["source_sha256"].as_str().unwrap();
        assert_eq!(
            format!("{:x}", Sha256::digest(source.as_bytes())),
            expected_source_sha
        );
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
            let expected = &case["expected"][target];
            let cell = cells
                .iter()
                .find(|cell| {
                    cell["case"].as_str() == case["name"].as_str()
                        && cell["python"].as_str() == Some(version.name())
                })
                .expect("missing frozen original log cell");
            assert_eq!(cell["original_id"], case["original_id"]);
            assert_eq!(cell["source_sha256"].as_str(), Some(expected_source_sha));
            assert_eq!(cell["reference_exit"], expected["reference_exit"]);
            assert_eq!(cell["exception_type"], expected["exception_type"]);
            let (expected_prefix, expected_log_path) =
                message_parts(expected["message"].as_str().unwrap())
                    .expect("invalid original expected message path");
            let expected_message_prefix = format!("{expected_prefix}{MESSAGE_MARKER}");
            assert_eq!(
                cell["expected_message_prefix"].as_str(),
                Some(expected_message_prefix.as_str())
            );
            assert_eq!(
                cell["original_expected_log_parent"].as_str(),
                expected_log_path.parent().unwrap().to_str()
            );
            let expected_log = decode_hex(cell["log_bytes_hex"].as_str().unwrap())
                .expect("invalid frozen original log bytes");
            let expected_log_sha = cell["log_sha256"].as_str().unwrap();
            assert_eq!(
                expected_log.len(),
                cell["log_size"].as_u64().unwrap() as usize
            );
            assert_eq!(
                format!("{:x}", Sha256::digest(&expected_log)),
                expected_log_sha
            );
            assert_eq!(cell["repeat_log_sha256"], cell["log_sha256"]);

            let source_before_format = fs::read(&path).unwrap();
            let actual = format_record(format(&workspace.sources[0], &workspace));
            let source_after_format = fs::read(&path).unwrap();
            let actual_message = actual["error"].as_str();
            let actual_log = log_record(actual_message);
            let actual_path_matches =
                actual_message
                    .and_then(message_parts)
                    .is_some_and(|(_, actual_path)| {
                        canonical_log_parent_matches_current_tmp(actual_path)
                    });
            let actual_message_matches = actual_message.is_some_and(|message| {
                message_equal(message, expected["message"].as_str().unwrap())
            });
            let actual_log_matches = actual_log["bytes"]["bytes_hex"] == cell["log_bytes_hex"]
                && actual_log["bytes"]["sha256"] == cell["log_sha256"]
                && actual_log["bytes"]["size"] == cell["log_size"];
            let actual_matches = actual["kind"] == expected["kind"]
                && actual["error_class"] == expected["exception_type"]
                && actual["output"] == expected["output"]
                && actual_message_matches
                && actual_path_matches
                && actual_log_matches;

            let source_before_repeat = fs::read(&path).unwrap();
            let repeated = format_record(format(&workspace.sources[0], &workspace));
            let source_after_repeat = fs::read(&path).unwrap();
            let repeat_message = repeated["error"].as_str();
            let repeat_log = log_record(repeat_message);
            let repeat_path_matches =
                repeat_message
                    .and_then(message_parts)
                    .is_some_and(|(_, actual_path)| {
                        canonical_log_parent_matches_current_tmp(actual_path)
                    });
            let repeat_message_matches = repeat_message.is_some_and(|message| {
                message_equal(message, expected["message"].as_str().unwrap())
            });
            let repeat_log_matches = repeat_log["bytes"]["bytes_hex"] == cell["log_bytes_hex"]
                && repeat_log["bytes"]["sha256"] == cell["repeat_log_sha256"]
                && repeat_log["bytes"]["size"] == cell["log_size"];
            let repeated_matches = repeated["kind"] == expected["kind"]
                && repeated["error_class"] == expected["exception_type"]
                && repeated["output"] == expected["output"]
                && repeat_message_matches
                && repeat_path_matches
                && repeat_log_matches;

            let source_before_check = fs::read(&path).unwrap();
            let report = check(&workspace);
            let source_after_check = fs::read(&path).unwrap();
            let public_tool_report = serde_json::to_value(&report).unwrap();
            let public_error_logs = report_log_records(&report);
            let diagnostic = report.diagnostics.first();
            let public_report_matches = report.tool == QualityTool::Black
                && report.reference_version == "26.10.0"
                && report.completion == CompletionStatus::Error
                && report.reference_exit_code.is_none()
                && report.checked_files == 0
                && report.diagnostics.len() == 1
                && diagnostic.is_some_and(|d| {
                    d.tool == QualityTool::Black
                        && d.rule == expected["exception_type"].as_str().unwrap()
                        && message_equal(&d.message, expected["message"].as_str().unwrap())
                        && canonical_log_parent_matches_current_tmp(
                            message_parts(&d.message).unwrap().1,
                        )
                        && d.path == path
                        && d.severity == "error"
                        && d.hint.is_none()
                        && d.location.line == 1
                        && d.location.column == 1
                        && d.location.end_line.is_none()
                        && d.location.end_column.is_none()
                        && d.location.offset.is_none()
                        && d.location.end_offset.is_none()
                        && d.fix.is_none()
                });
            let public_log_matches = public_error_logs.len() == 1
                && public_error_logs[0]["artifact"]["bytes"]["bytes_hex"] == cell["log_bytes_hex"]
                && public_error_logs[0]["artifact"]["bytes"]["sha256"] == cell["log_sha256"];

            let source_unchanged_after_format = source_unchanged(
                &source_before_format,
                &source_after_format,
                expected_source_sha,
            );
            let source_unchanged_after_repeat = source_unchanged(
                &source_before_repeat,
                &source_after_repeat,
                expected_source_sha,
            );
            let source_unchanged_after_check = source_unchanged(
                &source_before_check,
                &source_after_check,
                expected_source_sha,
            );
            let passed = actual_matches
                && repeated_matches
                && public_report_matches
                && public_log_matches
                && source_unchanged_after_format
                && source_unchanged_after_repeat
                && source_unchanged_after_check;
            failures += usize::from(!passed);

            let mut row = Map::new();
            row.insert("case".into(), case["name"].clone());
            row.insert("python".into(), json!(version.name()));
            row.insert("source_sha256".into(), json!(expected_source_sha));
            row.insert(
                "computed_reference_exit".into(),
                expected["reference_exit"].clone(),
            );
            row.insert(
                "computed_reference_exit_semantics".into(),
                json!("frozen original comparison value; not this native call's process exit"),
            );
            row.insert("native_process_exit_observed".into(), json!(false));
            row.insert("expected".into(), expected.clone());
            row.insert("actual_format".into(), actual);
            row.insert("actual_log_artifact".into(), actual_log);
            row.insert("actual_matches".into(), json!(actual_matches));
            row.insert("repeat_format".into(), repeated);
            row.insert("repeat_log_artifact".into(), repeat_log);
            row.insert("repeat_matches".into(), json!(repeated_matches));
            row.insert("public_tool_report".into(), public_tool_report);
            row.insert(
                "public_error_log_artifacts".into(),
                Value::Array(public_error_logs),
            );
            row.insert("public_report_matches".into(), json!(public_report_matches));
            row.insert("public_error_log_matches".into(), json!(public_log_matches));
            row.insert(
                "source_before_format".into(),
                bytes_record(&source_before_format),
            );
            row.insert(
                "source_after_format".into(),
                bytes_record(&source_after_format),
            );
            row.insert(
                "source_unchanged_after_format".into(),
                json!(source_unchanged_after_format),
            );
            row.insert(
                "source_before_repeat".into(),
                bytes_record(&source_before_repeat),
            );
            row.insert(
                "source_after_repeat".into(),
                bytes_record(&source_after_repeat),
            );
            row.insert(
                "source_unchanged_after_repeat".into(),
                json!(source_unchanged_after_repeat),
            );
            row.insert(
                "source_before_check".into(),
                bytes_record(&source_before_check),
            );
            row.insert(
                "source_after_check".into(),
                bytes_record(&source_after_check),
            );
            row.insert(
                "source_unchanged_after_check".into(),
                json!(source_unchanged_after_check),
            );
            row.insert("passed".into(), json!(passed));
            rows.push(Value::Object(row));
        }
    }

    if let Some(destination) = std::env::var_os("MAMBA_QUALITY_BLACK_SAFETY_EVIDENCE") {
        let evidence = json!({
            "schema":"mamba.quality.black-safety-comparison.v2",
            "safety_fixture_sha256":format!("{:x}",Sha256::digest(corpus.as_bytes())),
            "safety_log_fixture_sha256":format!("{:x}",Sha256::digest(logs_text.as_bytes())),
            "cells":rows,
            "failed":failures,
            "whole_suite_accepted":false
        });
        fs::write(destination, serde_json::to_vec_pretty(&evidence).unwrap()).unwrap();
    }
    assert_eq!(failures, 0, "Pinned Black safety comparisons failed");
}
