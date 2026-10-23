//! Six strict Mypy with-target boundary controls from Root-qualified originals.
use crate::domain::{model::CompletionStatus, ports::ProjectStore, source::PythonVersion};
use crate::infrastructure::filesystem::Filesystem;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, fs, io::Write};

const FIXTURE_SHA256: &str = "c61286ec7dec6806caa1b8745d44fd1b904d909a9d0d6d43bc96f8d7f76dbc1e";
const SOURCE_INVENTORY_SHA256: &str =
    "a0c3864527bc96b57cc82303993b20f21497d02b710c65c4f41bbbb3a5a1cfe9";
const ORIGINAL_CAPTURE_MANIFEST_SHA256: &str =
    "f1f5ffab0239bdbec31e4990e27489e3e8aac87ef2465153427a540bce23180c";
const ORIGINAL_VERIFICATION_SHA256: &str =
    "d73e925969c5f9d49ebccf93d125f400b57e71b6af3dd6c85d9a07981e016749";
const ROOT_REVIEW_SHA256: &str = "c3c0e5bf17cc574e0d32dcd7b9473c26a7d0f94a9d5640c2c94193b1cd7f3395";
const EXPECTED_REMAINING: [&str; 3] = [
    "mypy full strict inference is incomplete: symbol resolution, callable-value types, unpacked arguments, coroutine and generator results, containers, generics, overloads, Protocol, TypedDict, ParamSpec, TypeVarTuple, narrowing, inheritance, dataclass, and cross-file constraints",
    "generic type aliases remain incomplete: recursive expansion, complete scopes, forward references, nested binders, ParamSpec, and TypeVarTuple",
    "Pydantic 2.13.5 model construction, aliases, generic substitution, frozen fields, defaults, and dynamic-alias warnings remain incomplete",
];
const EXPECTED_CASES: [&str; 6] = [
    "next_context_from_target",
    "manager_reused_as_target",
    "tuple_enter_unpack",
    "none_enter_result",
    "incompatible_predeclared_target",
    "undefined_context_expression",
];

#[test]
fn pinned_with_boundary_originals() {
    let fixture_bytes = include_bytes!("with_boundaries.json");
    assert_eq!(sha256(fixture_bytes), FIXTURE_SHA256);
    let fixture: Value = serde_json::from_slice(fixture_bytes).unwrap();
    assert_eq!(
        fixture["schema"],
        "mamba.quality.mypy.with-boundaries-protected.v1"
    );
    assert_eq!(fixture["source_inventory_sha256"], SOURCE_INVENTORY_SHA256);
    assert_eq!(
        fixture["original_capture_manifest_sha256"],
        ORIGINAL_CAPTURE_MANIFEST_SHA256
    );
    assert_eq!(
        fixture["original_verification_sha256"],
        ORIGINAL_VERIFICATION_SHA256
    );
    assert_eq!(
        fixture["root_raw_independent_review_sha256"],
        ROOT_REVIEW_SHA256
    );
    assert_eq!(fixture["selected_flag_state"], "extra_checks_enabled");
    assert_eq!(fixture["targets"], json!(["3.12", "3.13", "3.14"]));
    assert_eq!(fixture["cell_count"], 18);
    assert_eq!(fixture["original_calls"], 72);
    assert_eq!(fixture["repeat_pairs"], 36);
    assert_eq!(fixture["whole_acceptance"], false);

    let cells = fixture["cells"].as_array().unwrap();
    assert_eq!(cells.len(), 18);
    let mut identities = BTreeSet::new();
    let mut seen_cases = BTreeSet::new();
    let mut failed = 0usize;
    let mut rows = Vec::with_capacity(cells.len());

    for cell in cells {
        let case = cell["case"].as_str().unwrap();
        let target = cell["target"].as_str().unwrap();
        assert!(identities.insert((case.to_owned(), target.to_owned())));
        seen_cases.insert(case.to_owned());
        assert_eq!(cell["filename"], "main.py");
        assert_eq!(cell["flag_state"], "extra_checks_enabled");
        assert_eq!(cell["json_start_column_public_mapping"], "original JSON start column is zero-based; public native column is original + 1; end column is unchanged");
        let version = match target {
            "3.12" => PythonVersion::Py312,
            "3.13" => PythonVersion::Py313,
            "3.14" => PythonVersion::Py314,
            other => panic!("Unexpected target in frozen fixture: {other}"),
        };
        let source = cell["source"].as_str().unwrap().as_bytes();
        let source_hash = sha256(source);
        assert_eq!(cell["source_sha256"].as_str(), Some(source_hash.as_str()));
        assert_eq!(cell["source_bytes"].as_u64(), Some(source.len() as u64));
        assert_eq!(
            cell["source_bytes_hex"].as_str(),
            Some(hex(source).as_str())
        );
        assert_eq!(cell["source_mode"].as_u64(), Some(0o644));

        let raw_calls = cell["raw_original_calls"].as_array().unwrap();
        assert_eq!(raw_calls.len(), 4);
        for reporter in ["json", "text"] {
            let pair: Vec<_> = raw_calls
                .iter()
                .filter(|call| call["reporter"] == reporter)
                .collect();
            assert_eq!(pair.len(), 2);
            for (idx, call) in pair.iter().enumerate() {
                assert_eq!(call["repeat"].as_u64(), Some((idx + 1) as u64));
                assert_eq!(call["target"], target);
                assert_eq!(call["flag_state"], "extra_checks_enabled");
                assert_eq!(call["timed_out"], false);
                assert_eq!(call["process_state"], "completed");
                assert_eq!(call["process_group_cleanup_incomplete"], false);
                assert_eq!(call["spawn_error"], Value::Null);
                assert_eq!(call["timeout_seconds"], 120);
                let stdout = unhex(call["stdout_hex"].as_str().unwrap());
                let stderr = unhex(call["stderr_hex"].as_str().unwrap());
                assert_eq!(stdout.len() as u64, call["stdout_bytes"].as_u64().unwrap());
                assert_eq!(stderr.len() as u64, call["stderr_bytes"].as_u64().unwrap());
                assert_eq!(sha256(&stdout), call["stdout_sha256"].as_str().unwrap());
                assert_eq!(sha256(&stderr), call["stderr_sha256"].as_str().unwrap());
                assert_eq!(call["actual_exit"], call["actual_subprocess_returncode"]);
            }
            assert_eq!(pair[0]["actual_exit"], pair[1]["actual_exit"]);
            assert_eq!(pair[0]["stdout_hex"], pair[1]["stdout_hex"]);
            assert_eq!(pair[0]["stderr_hex"], pair[1]["stderr_hex"]);
            assert_eq!(pair[0]["stdout_sha256"], pair[1]["stdout_sha256"]);
            assert_eq!(pair[0]["stderr_sha256"], pair[1]["stderr_sha256"]);
        }
        let json_calls: Vec<_> = raw_calls
            .iter()
            .filter(|call| call["reporter"] == "json")
            .collect();
        let text_calls: Vec<_> = raw_calls
            .iter()
            .filter(|call| call["reporter"] == "text")
            .collect();
        let json_exit = cell["original_json_exit"].as_i64().unwrap();
        let text_exit = cell["original_text_exit"].as_i64().unwrap();
        assert_eq!(json_calls[0]["actual_exit"].as_i64(), Some(json_exit));
        assert_eq!(text_calls[0]["actual_exit"].as_i64(), Some(text_exit));
        assert_eq!(json_exit, text_exit);

        // Parse the frozen JSON stream. Mypy emits one JSON object per diagnostic line.
        let json_stdout = unhex(json_calls[0]["stdout_hex"].as_str().unwrap());
        let parsed_json: Vec<Value> = if json_stdout.iter().all(u8::is_ascii_whitespace) {
            Vec::new()
        } else {
            json_stdout
                .split(|byte| *byte == b'\n')
                .filter(|line| !line.iter().all(u8::is_ascii_whitespace))
                .map(|line| serde_json::from_slice(line).unwrap())
                .collect()
        };
        assert_eq!(
            &parsed_json,
            cell["original_json_diagnostics"].as_array().unwrap()
        );
        let text_stdout = unhex(text_calls[0]["stdout_hex"].as_str().unwrap());
        assert_eq!(
            text_stdout,
            unhex(cell["original_text_stdout_hex"].as_str().unwrap())
        );
        let text_lines: Vec<String> = String::from_utf8(text_stdout.clone())
            .unwrap()
            .lines()
            .map(str::to_owned)
            .collect();
        assert_eq!(
            text_lines,
            serde_json::from_value::<Vec<String>>(cell["original_text_lines"].clone()).unwrap()
        );
        let text_diags: Vec<Value> = cell["original_text_diagnostics"]
            .as_array()
            .unwrap()
            .to_vec();
        assert_eq!(
            text_lines,
            text_calls[0]["parsed_surface"]
                .as_array()
                .unwrap()
                .iter()
                .map(|value| value.as_str().unwrap().to_owned())
                .collect::<Vec<_>>()
        );
        let expected_json: Vec<Value> = parsed_json
            .iter()
            .map(|diag| {
                let column = diag["column"].as_u64().unwrap();
                json!({
                    "rule": diag["code"],
                    "message": diag["message"],
                    "hint": diag["hint"],
                    "path": diag["file"],
                    "severity": diag["severity"],
                    "location": {
                        "line": diag["line"],
                        "column": column + 1,
                        "end_line": diag["end_line"],
                        "end_column": diag["end_column"],
                    }
                })
            })
            .collect();
        assert_eq!(
            cell["expected_native_diagnostics"],
            Value::Array(expected_json.clone())
        );
        assert_eq!(text_diags.len(), parsed_json.len());
        let derived_text_lines: Vec<String> = parsed_json
            .iter()
            .map(|diag| {
                format!(
                    "{}:{}:{}:{}:{}: {}: {}  [{}]",
                    diag["file"].as_str().unwrap(),
                    diag["line"].as_u64().unwrap(),
                    diag["column"].as_u64().unwrap() + 1,
                    diag["end_line"].as_u64().unwrap(),
                    diag["end_column"].as_u64().unwrap(),
                    diag["severity"].as_str().unwrap(),
                    diag["message"].as_str().unwrap(),
                    diag["code"].as_str().unwrap()
                )
            })
            .collect();
        assert_eq!(text_lines, derived_text_lines);
        for (json_diag, text_diag) in parsed_json.iter().zip(text_diags.iter()) {
            assert_eq!(text_diag["file"], json_diag["file"]);
            assert_eq!(text_diag["line"], json_diag["line"]);
            assert_eq!(
                text_diag["column"].as_u64(),
                json_diag["column"].as_u64().map(|c| c + 1)
            );
            assert_eq!(text_diag["end_line"], json_diag["end_line"]);
            assert_eq!(text_diag["end_column"], json_diag["end_column"]);
            assert_eq!(text_diag["message"], json_diag["message"]);
            assert_eq!(text_diag["severity"], json_diag["severity"]);
            assert_eq!(text_diag["code"], json_diag["code"]);
        }

        let root = tempfile::tempdir().unwrap();
        let root_path = fs::canonicalize(root.path()).unwrap();
        let path = root_path.join("main.py");
        fs::write(&path, source).unwrap();
        let before = fs::read(&path).unwrap();
        let mut workspace = Filesystem.load(std::slice::from_ref(&path), true);
        assert_eq!(workspace.projects.len(), 1);
        workspace.projects[0].python_version = version;
        let report = super::super::check(&workspace);
        let after = fs::read(&path).unwrap();
        let actual: Vec<_> = report.diagnostics.iter().map(|diagnostic| json!({
            "rule": diagnostic.rule, "message": diagnostic.message, "hint": diagnostic.hint,
            "path": diagnostic.path.strip_prefix(&root_path).unwrap(), "severity": diagnostic.severity,
            "location": {"line": diagnostic.location.line, "column": diagnostic.location.column,
                "end_line": diagnostic.location.end_line, "end_column": diagnostic.location.end_column},
        })).collect();
        let expected = cell["expected_native_diagnostics"].clone();
        let diagnostics_match = Value::Array(actual.clone()) == expected;
        let computed_status = i32::from(report.diagnostics.iter().any(|d| d.severity == "error"));
        let exits_match =
            i64::from(computed_status) == json_exit && i64::from(computed_status) == text_exit;
        let source_preserved = before == source && after == source;
        let report_json = serde_json::to_value(&report).unwrap();
        let remaining_matches = report
            .remaining
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>()
            == EXPECTED_REMAINING;
        let contract_matches = report.completion == CompletionStatus::Incomplete
            && report.reference_exit_code.is_none()
            && report.checked_files == 1
            && remaining_matches;
        let matched = diagnostics_match && exits_match && source_preserved && contract_matches;
        failed += usize::from(!matched);
        rows.push(json!({
            "case": case, "target": target, "flag_state": "extra_checks_enabled",
            "source_path": path, "source_sha256": source_hash,
            "source_before_hex": hex(&before), "source_after_hex": hex(&after),
            "source_preserved": source_preserved,
            "original_json_exit": json_exit, "original_text_exit": text_exit,
            "original_json_diagnostics": cell["original_json_diagnostics"],
            "original_text_lines": cell["original_text_lines"],
            "original_text_diagnostics": cell["original_text_diagnostics"],
            "raw_original_calls": raw_calls,
            "expected_native_diagnostics": expected, "actual_diagnostics": actual,
            "computed_diagnostic_status": computed_status,
            "actual_tool_report": report_json,
            "actual_completion": report.completion,
            "actual_reference_exit_code": report.reference_exit_code,
            "actual_checked_files": report.checked_files,
            "actual_remaining": report.remaining,
            "diagnostics_match": diagnostics_match, "exits_match": exits_match,
            "remaining_matches": remaining_matches, "contract_matches": contract_matches,
            "unobservable_native_fields": cell["native_unobservable_fields"],
            "matched": matched,
        }));
    }

    if let Some(destination) = std::env::var_os("MAMBA_QUALITY_MYPY_WITH_BOUNDARIES_EVIDENCE") {
        let evidence = json!({
            "schema": "mamba.quality.mypy.with-boundaries-comparison.v1",
            "fixture_sha256": FIXTURE_SHA256,
            "source_inventory_sha256": SOURCE_INVENTORY_SHA256,
            "original_capture_manifest_sha256": ORIGINAL_CAPTURE_MANIFEST_SHA256,
            "original_verification_sha256": ORIGINAL_VERIFICATION_SHA256,
            "root_raw_independent_review_sha256": ROOT_REVIEW_SHA256,
            "rows": rows,
            "failed": failed,
            "native_reference_complete": false,
            "whole_acceptance": false,
        });
        let mut output = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(destination)
            .unwrap();
        output
            .write_all(&serde_json::to_vec_pretty(&evidence).unwrap())
            .unwrap();
    }
    assert_eq!(identities.len(), 18);
    assert_eq!(
        seen_cases,
        EXPECTED_CASES.into_iter().map(str::to_owned).collect()
    );
    assert_eq!(rows.len(), 18);
    assert_eq!(
        failed, 0,
        "Mypy with-boundary diagnostics differ from frozen originals"
    );
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
fn unhex(value: &str) -> Vec<u8> {
    assert_eq!(value.len() % 2, 0);
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let digit = |byte: u8| match byte {
                b'0'..=b'9' => byte - b'0',
                b'a'..=b'f' => byte - b'a' + 10,
                _ => panic!("invalid hex"),
            };
            digit(pair[0]) * 16 + digit(pair[1])
        })
        .collect()
}
