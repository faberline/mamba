//! Lambda scope boundary behavior from qualified Mypy 2.4.0 originals.
use crate::domain::{model::CompletionStatus, ports::ProjectStore, source::PythonVersion};
use crate::infrastructure::filesystem::Filesystem;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, fs, io::Write};

const FIXTURE_SHA256: &str = "322e31022b626fe404ae180066b4f5599256690f06ff7be2e4f5da3fca1aec13";
const ORIGINAL_CAPTURE_SHA256: &str =
    "8b544198748a817d8988545e44a1e9b988d081e2cfe610084e5d5a4878297c01";
const ORIGINAL_VERIFICATION_SHA256: &str =
    "f4850042c601d417b0ca4949ce00585d21e73f7d73b1c889aa680886cd8aad03";
const EXPECTED_REMAINING: [&str; 3] = [
    "mypy full strict inference is incomplete: symbol resolution, callable-value types, unpacked arguments, coroutine and generator results, containers, generics, overloads, Protocol, TypedDict, ParamSpec, TypeVarTuple, narrowing, inheritance, dataclass, and cross-file constraints",
    "generic type aliases remain incomplete: recursive expansion, complete scopes, forward references, nested binders, ParamSpec, and TypeVarTuple",
    "Pydantic 2.13.5 model construction, aliases, generic substitution, frozen fields, defaults, and dynamic-alias warnings remain incomplete",
];
const EXPECTED_CASES: [&str; 3] = [
    "lambda_default_outer_name",
    "lambda_parameter_kinds",
    "lambda_nested_scopes",
];

#[test]
fn pinned_lambda_scope_boundary_originals() {
    let fixture_bytes = include_bytes!("lambda_scope_boundaries.json");
    assert_eq!(sha256(fixture_bytes), FIXTURE_SHA256);
    let fixture: Value = serde_json::from_slice(fixture_bytes).unwrap();
    assert_eq!(
        fixture["schema"],
        "mamba.quality.mypy.lambda-scope-boundaries.v1"
    );
    assert_eq!(
        fixture["source_inventory_sha256"],
        "550617d6678e0b51c4672cc85ab5993df4d6f7e7a464c36ddf4754d13934bd0e"
    );
    assert_eq!(
        fixture["original_capture_manifest_sha256"],
        ORIGINAL_CAPTURE_SHA256
    );
    assert_eq!(
        fixture["original_capture_verification_sha256"],
        ORIGINAL_VERIFICATION_SHA256
    );
    assert_eq!(
        fixture["root_raw_independent_review_sha256"],
        "6df37cc84f9f00fd103152e7c11d27068bb4332efed454923fdcc8c05fc1eb5e"
    );
    assert_eq!(fixture["selected_flag_state"], "extra_checks_enabled");
    assert_eq!(fixture["cell_count"], 9);
    assert_eq!(fixture["original_calls"], 36);
    assert_eq!(fixture["repeat_pairs"], 18);
    assert_eq!(fixture["whole_acceptance"], false);

    let cells = fixture["cells"].as_array().unwrap();
    assert_eq!(cells.len(), 9);
    let mut identities = BTreeSet::new();
    let mut seen_cases = BTreeSet::new();
    let mut failures = 0usize;
    let mut rows = Vec::with_capacity(cells.len());

    for cell in cells {
        let case = cell["case"].as_str().unwrap();
        let target = cell["target"].as_str().unwrap();
        assert!(identities.insert((case.to_owned(), target.to_owned())));
        seen_cases.insert(case.to_owned());
        assert_eq!(cell["filename"], "main.py");
        assert_eq!(cell["flag_state"], "extra_checks_enabled");
        let version = match target {
            "3.12" => PythonVersion::Py312,
            "3.13" => PythonVersion::Py313,
            "3.14" => PythonVersion::Py314,
            other => panic!("unexpected target in frozen fixture: {other}"),
        };
        let source = cell["source"].as_str().unwrap().as_bytes();
        let source_hash = sha256(source);
        assert_eq!(cell["source_sha256"].as_str(), Some(source_hash.as_str()));
        assert_eq!(cell["source_bytes"].as_u64(), Some(source.len() as u64));
        assert_eq!(
            cell["source_bytes_hex"].as_str(),
            Some(hex(source).as_str())
        );

        // Original process results are independent evidence. Keep JSON and text exits separate.
        let raw_calls = cell["raw_original_calls"].as_array().unwrap();
        assert_eq!(raw_calls.len(), 4);
        for reporter in ["json", "text"] {
            let pair: Vec<_> = raw_calls
                .iter()
                .filter(|call| call["reporter"] == reporter)
                .collect();
            assert_eq!(pair.len(), 2);
            assert_eq!(pair[0]["repeat"], 1);
            assert_eq!(pair[1]["repeat"], 2);
            assert_eq!(pair[0]["actual_exit"], 1);
            assert_eq!(pair[1]["actual_exit"], 1);
            assert_eq!(pair[0]["normal_wait"], true);
            assert_eq!(pair[1]["normal_wait"], true);
            assert_eq!(pair[0]["stdout_hex"], pair[1]["stdout_hex"]);
            assert_eq!(pair[0]["stderr_hex"], pair[1]["stderr_hex"]);
            assert_eq!(pair[0]["stdout_sha256"], pair[1]["stdout_sha256"]);
            assert_eq!(pair[0]["stderr_sha256"], pair[1]["stderr_sha256"]);
            for call in &pair {
                let stdout = unhex(call["stdout_hex"].as_str().unwrap());
                let stderr = unhex(call["stderr_hex"].as_str().unwrap());
                assert_eq!(stdout.len() as u64, call["stdout_bytes"].as_u64().unwrap());
                assert_eq!(stderr.len() as u64, call["stderr_bytes"].as_u64().unwrap());
                let stdout_sha = sha256(&stdout);
                let stderr_sha = sha256(&stderr);
                assert_eq!(call["stdout_sha256"].as_str(), Some(stdout_sha.as_str()));
                assert_eq!(call["stderr_sha256"].as_str(), Some(stderr_sha.as_str()));
                assert_eq!(call["timeout_seconds"], 120);
                assert!(call["id"].is_string() && call["process_record_sha256"].is_string());
                assert!(
                    call["call_freeze_sha256"].is_string()
                        && call["checker_imports_sha256"].is_string()
                );
            }
        }
        let original_json_exit = cell["original_json_exit"].as_i64().unwrap();
        let original_text_exit = cell["original_text_exit"].as_i64().unwrap();
        assert_eq!(original_json_exit, 1);
        assert_eq!(original_text_exit, 1);

        // Bind parsed original rows to the captured stdout bytes, including the note line.
        let json_call = raw_calls
            .iter()
            .find(|call| call["reporter"] == "json" && call["repeat"] == 1)
            .unwrap();
        let json_stdout = unhex(json_call["stdout_hex"].as_str().unwrap());
        let parsed_json: Value = serde_json::from_slice(&json_stdout).unwrap();
        assert_eq!(parsed_json, cell["original_json_diagnostics"][0]);
        let text_call = raw_calls
            .iter()
            .find(|call| call["reporter"] == "text" && call["repeat"] == 1)
            .unwrap();
        let text_stdout = unhex(text_call["stdout_hex"].as_str().unwrap());
        let expected_text_stdout = format!(
            "{}\n{}\n",
            cell["original_text_error"]["raw_line"].as_str().unwrap(),
            cell["original_text_note"]["raw_line"].as_str().unwrap()
        );
        assert_eq!(text_stdout, expected_text_stdout.as_bytes());

        // Derive the expected public diagnostic from the original JSON and text error row.
        let json_diagnostics = cell["original_json_diagnostics"].as_array().unwrap();
        assert_eq!(json_diagnostics.len(), 1);
        let original_json = &json_diagnostics[0];
        let text_error = &cell["original_text_error"];
        let text_note = &cell["original_text_note"];
        assert_eq!(text_error["kind"], "error");
        assert_eq!(text_error["file"], cell["filename"]);
        assert_eq!(text_error["code"], original_json["code"]);
        assert_eq!(text_error["message"], original_json["message"]);
        assert_eq!(text_note["kind"], "note");
        assert_eq!(text_note["message"], original_json["hint"]);
        assert_eq!(text_note["file"], text_error["file"]);
        assert_eq!(text_note["line"], text_error["line"]);
        assert_eq!(text_note["column"], text_error["column"]);
        assert_eq!(text_note["end_line"], text_error["end_line"]);
        assert!(text_note["raw_line"]
            .as_str()
            .unwrap()
            .contains(text_note["message"].as_str().unwrap()));
        let expected = json!([{
            "rule": original_json["code"],
            "message": original_json["message"],
            "hint": original_json["hint"],
            "path": original_json["file"],
            "severity": original_json["severity"],
            "location": {
                "line": text_error["line"],
                "column": text_error["column"],
                "end_line": text_error["end_line"],
                "end_column": text_error["end_column"],
            }
        }]);
        assert_eq!(cell["expected_native_diagnostics"], expected);
        assert_eq!(
            cell["native_unobservable_fields"],
            json!(["offset", "end_offset", "fix", "separate_note_location"])
        );

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
                    }
                })
            })
            .collect();
        let diagnostics_match = Value::Array(actual.clone()) == expected;
        let computed_error_status =
            i32::from(report.diagnostics.iter().any(|d| d.severity == "error"));
        let status_matches = computed_error_status == original_text_exit as i32;
        let contract_matches = report.completion == CompletionStatus::Incomplete
            && report.reference_exit_code.is_none()
            && report.checked_files == 1
            && report
                .remaining
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>()
                == EXPECTED_REMAINING;
        let source_preserved = before == source && after == source;
        let matched = diagnostics_match && status_matches && contract_matches && source_preserved;
        failures += usize::from(!matched);
        rows.push(json!({
            "case": case,
            "target": target,
            "flag_state": cell["flag_state"],
            "source_path": path,
            "source_sha256": source_hash,
            "source_before_hex": hex(&before),
            "source_after_hex": hex(&after),
            "source_preserved": source_preserved,
            "original_json_exit": original_json_exit,
            "original_text_exit": original_text_exit,
            "original_json_exit_is_provenance_only": true,
            "original_json_diagnostics": json_diagnostics,
            "original_text_error": text_error,
            "original_text_note": text_note,
            "raw_original_calls": raw_calls,
            "expected_native_diagnostics": expected,
            "actual_diagnostics": actual,
            "computed_error_status": computed_error_status,
            "native_tool_report": serde_json::to_value(&report).unwrap(),
            "native_completion": report.completion,
            "native_reference_exit_code": report.reference_exit_code,
            "native_checked_files": report.checked_files,
            "native_remaining": report.remaining,
            "diagnostics_match": diagnostics_match,
            "status_matches_original_text_exit": status_matches,
            "contract_matches": contract_matches,
            "unobservable_native_fields": cell["native_unobservable_fields"],
            "matched": matched,
        }));
    }

    assert_eq!(identities.len(), 9);
    assert_eq!(
        seen_cases,
        EXPECTED_CASES.into_iter().map(str::to_owned).collect()
    );
    assert_eq!(rows.len(), 9);
    if let Some(destination) =
        std::env::var_os("MAMBA_QUALITY_MYPY_LAMBDA_SCOPE_BOUNDARIES_EVIDENCE")
    {
        let evidence = json!({
            "schema": "mamba.quality.mypy.lambda-scope-boundaries-comparison.v1",
            "fixture_sha256": sha256(fixture_bytes),
            "original_capture_manifest_sha256": ORIGINAL_CAPTURE_SHA256,
            "original_capture_verification_sha256": ORIGINAL_VERIFICATION_SHA256,
            "rows": rows,
            "failed": failures,
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
    assert_eq!(
        failures, 0,
        "Lambda scope boundary diagnostics differ from frozen originals"
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
                _ => panic!("invalid lowercase hex digit"),
            };
            digit(pair[0]) * 16 + digit(pair[1])
        })
        .collect()
}
