//! Call spans from qualified Mypy originals; JSON and text exits stay separate.
use crate::domain::{model::CompletionStatus, ports::ProjectStore, source::PythonVersion};
use crate::infrastructure::filesystem::Filesystem;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, fs, io::Write};

const FIXTURE_SHA256: &str = "955ac8fc844f760fadc2cc04f6808a076cfdbaff8ecba2defb8c70210e531fe9";
const SOURCES_SHA256: &str = "e15e6aeb28ffc33a83c609ff93a22b188042113b29f337f7f64af19ba87728a9";
const CAPTURE_SHA256: &str = "e37566f0a25f7d63fc19c668f838533f00e7a10cf30c8221a1ac5e718f32f234";
const VERIFICATION_SHA256: &str =
    "613fc3dfa57f3ad0d1d78e4f51d18a9e879b3a47966871ec394fd1d0beb99c0b";
const ROOT_REVIEW_SHA256: &str = "36d1d9213f4fc5cbaee6e717858b28a8b990e0f68971d9c193117b81f268e5f5";
const CASES: [&str; 5] = [
    "typed_callable_factory",
    "three_deep_untyped_lambda",
    "sibling_untyped_lambdas",
    "different_named_callables",
    "nested_inline_ignore",
];
const PROFILE_FLAGS: [&str; 13] = [
    "--disallow-any-generics",
    "--disallow-subclassing-any",
    "--disallow-untyped-calls",
    "--disallow-untyped-defs",
    "--disallow-incomplete-defs",
    "--check-untyped-defs",
    "--disallow-untyped-decorators",
    "--warn-redundant-casts",
    "--warn-unused-ignores",
    "--warn-return-any",
    "--no-implicit-reexport",
    "--strict-equality",
    "--extra-checks",
];
const REMAINING: [&str; 3] = [
    "mypy full strict inference is incomplete: symbol resolution, callable-value types, unpacked arguments, coroutine and generator results, containers, generics, overloads, Protocol, TypedDict, ParamSpec, TypeVarTuple, narrowing, inheritance, dataclass, and cross-file constraints",
    "generic type aliases remain incomplete: recursive expansion, complete scopes, forward references, nested binders, ParamSpec, and TypeVarTuple",
    "Pydantic 2.13.5 model construction, aliases, generic substitution, frozen fields, defaults, and dynamic-alias warnings remain incomplete",
];

#[test]
fn pinned_original_call_span_boundaries() {
    let bytes = include_bytes!("call_span_boundaries.json");
    assert_eq!(sha256(bytes), FIXTURE_SHA256);
    let fixture: Value = serde_json::from_slice(bytes).unwrap();
    assert_eq!(
        fixture["schema"],
        "mamba.quality.mypy.call-span-boundaries.v1"
    );
    assert_eq!(fixture["sources_sha256"], SOURCES_SHA256);
    assert_eq!(fixture["original_capture_manifest_sha256"], CAPTURE_SHA256);
    assert_eq!(
        fixture["original_capture_verification_sha256"],
        VERIFICATION_SHA256
    );
    assert_eq!(
        fixture["root_raw_independent_review_sha256"],
        ROOT_REVIEW_SHA256
    );
    assert_eq!(fixture["original_calls"], 60);
    assert_eq!(fixture["repeat_pairs"], 30);
    assert_eq!(fixture["cell_count"], 15);
    assert_eq!(fixture["profile_flags"], json!(PROFILE_FLAGS));
    assert_eq!(fixture["native_reference_complete"], false);
    assert_eq!(fixture["whole_acceptance"], false);
    let cells = fixture["cells"].as_array().unwrap();
    let mut identities = BTreeSet::new();
    let mut rows = Vec::with_capacity(cells.len());
    let mut failed = 0usize;
    for cell in cells {
        let case = cell["case"].as_str().unwrap();
        let target = cell["target"].as_str().unwrap();
        let unique = identities.insert((case.to_owned(), target.to_owned()));
        let version = match target {
            "3.12" => PythonVersion::Py312,
            "3.13" => PythonVersion::Py313,
            "3.14" => PythonVersion::Py314,
            other => panic!("unexpected frozen target: {other}"),
        };
        let source = cell["source"].as_str().unwrap().as_bytes();
        assert_eq!(cell["filename"], "main.py");
        assert_eq!(cell["source_sha256"], sha256(source));
        assert_eq!(cell["source_bytes"].as_u64(), Some(source.len() as u64));
        assert_eq!(cell["source_bytes_hex"], hex(source));
        assert_eq!(cell["profile_flags"], json!(PROFILE_FLAGS));
        assert_eq!(cell["flag_state"], "extra_checks_enabled");
        validate_originals(cell);
        let expected = derive_projection(cell);
        assert_eq!(cell["expected_native_diagnostics"], expected);
        let root = tempfile::tempdir().unwrap();
        let root_path = fs::canonicalize(root.path()).unwrap();
        let path = root_path.join("main.py");
        fs::write(&path, source).unwrap();
        let before = fs::read(&path).unwrap();
        let mut workspace = Filesystem.load(std::slice::from_ref(&path), true);
        let project_count = workspace.projects.len();
        if let Some(project) = workspace.projects.first_mut() {
            project.python_version = version;
        }
        let report = super::super::check(&workspace);
        let raw_report = serde_json::to_value(&report).unwrap();
        let after = fs::read(&path).unwrap();
        let actual: Vec<_> = report
            .diagnostics
            .iter()
            .map(|d| {
                let raw = serde_json::to_value(d).unwrap();
                let public_path = d.path.strip_prefix(&root_path).unwrap_or(&d.path);
                json!({"rule":raw["rule"],"message":d.message,"hint":d.hint,
                "path":public_path,"severity":d.severity,"location":{"line":d.location.line,
                    "column":d.location.column,"end_line":d.location.end_line,
                    "end_column":d.location.end_column}})
            })
            .collect();
        let diagnostics_match = Value::Array(actual.clone()) == expected;
        let computed_error_status =
            i64::from(report.diagnostics.iter().any(|d| d.severity == "error"));
        let status_matches = cell["original_text_exit"].as_i64() == Some(computed_error_status);
        let contract_matches = project_count == 1
            && report.completion == CompletionStatus::Incomplete
            && report.reference_exit_code.is_none()
            && report.checked_files == 1
            && report
                .remaining
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>()
                == REMAINING;
        let source_preserved = before == source && after == source;
        let matched =
            unique && diagnostics_match && status_matches && contract_matches && source_preserved;
        failed += usize::from(!matched);
        rows.push(json!({"case":case,"target":target,"source_path":path,"source_sha256":sha256(source),
            "source_before_hex":hex(&before),"source_after_hex":hex(&after),"source_preserved":source_preserved,
            "raw_original_calls":cell["raw_original_calls"],"original_json_exit":cell["original_json_exit"],
            "original_text_exit":cell["original_text_exit"],"original_json_exit_is_provenance_only":true,
            "original_json_diagnostics":cell["original_json_diagnostics"],
            "original_text_diagnostics":cell["original_text_diagnostics"],
            "expected_native_diagnostics":expected,"actual_diagnostics":actual,
            "computed_error_status":computed_error_status,"status_matches_original_text_exit":status_matches,
            "native_tool_report":raw_report,"native_remaining":report.remaining,
            "diagnostics_match":diagnostics_match,"contract_matches":contract_matches,
            "unobservable_native_fields":cell["native_unobservable_fields"],"matched":matched}));
    }
    if let Some(destination) = std::env::var_os("MAMBA_QUALITY_MYPY_CALL_SPAN_BOUNDARIES_EVIDENCE")
    {
        let evidence = json!({"schema":"mamba.quality.mypy.call-span-boundaries-comparison.v1",
            "fixture_sha256":sha256(bytes),"sources_sha256":SOURCES_SHA256,
            "original_capture_manifest_sha256":CAPTURE_SHA256,
            "original_capture_verification_sha256":VERIFICATION_SHA256,
            "root_raw_independent_review_sha256":ROOT_REVIEW_SHA256,
            "rows":rows,"failed":failed,"native_reference_complete":false,"whole_acceptance":false});
        let mut output = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(destination)
            .unwrap();
        output
            .write_all(&serde_json::to_vec_pretty(&evidence).unwrap())
            .unwrap();
    }
    // Save all full reports before product parity assertions.
    let expected: BTreeSet<(String, String)> = CASES
        .into_iter()
        .flat_map(|case| {
            ["3.12", "3.13", "3.14"]
                .into_iter()
                .map(move |target| (case.to_owned(), target.to_owned()))
        })
        .collect();
    assert_eq!(identities, expected);
    assert_eq!(rows.len(), 15);
    assert_eq!(
        failed, 0,
        "Call-span diagnostics differ from qualified originals"
    );
}

fn validate_originals(cell: &Value) {
    let calls = cell["raw_original_calls"].as_array().unwrap();
    assert_eq!(calls.len(), 4);
    for reporter in ["json", "text"] {
        let pair: Vec<_> = calls.iter().filter(|c| c["reporter"] == reporter).collect();
        assert_eq!(pair.len(), 2);
        let exit_key = format!("original_{reporter}_exit");
        for (index, call) in pair.iter().enumerate() {
            assert_eq!(call["repeat"], index + 1);
            assert_eq!(call["actual_exit"], cell[exit_key.as_str()]);
            assert_eq!(call["normal_wait"], true);
            assert_eq!(call["timeout_seconds"], 120);
            assert_eq!(call["target"], cell["target"]);
            assert_eq!(call["case"], cell["case"]);
            assert_eq!(call["source_sha256"], cell["source_sha256"]);
            assert!(call["pid"].as_u64().is_some_and(|pid| pid > 0));
            for stream in ["stdout", "stderr"] {
                let key = format!("{stream}_hex");
                let raw = unhex(call[key.as_str()].as_str().unwrap());
                let key = format!("{stream}_sha256");
                assert_eq!(call[key.as_str()], sha256(&raw));
                let key = format!("{stream}_bytes");
                assert_eq!(call[key.as_str()].as_u64(), Some(raw.len() as u64));
                if stream == "stderr" {
                    assert!(raw.is_empty());
                }
            }
        }
        assert_eq!(pair[0]["stdout_hex"], pair[1]["stdout_hex"]);
        assert_eq!(pair[0]["stderr_hex"], pair[1]["stderr_hex"]);
    }
    let json_call = calls
        .iter()
        .find(|c| c["reporter"] == "json" && c["repeat"] == 1)
        .unwrap();
    let stdout = unhex(json_call["stdout_hex"].as_str().unwrap());
    let parsed: Vec<Value> = std::str::from_utf8(&stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(Value::Array(parsed), cell["original_json_diagnostics"]);
    let text_call = calls
        .iter()
        .find(|c| c["reporter"] == "text" && c["repeat"] == 1)
        .unwrap();
    let mut rendered = String::new();
    for row in cell["original_text_diagnostics"].as_array().unwrap() {
        let code = row["code"]
            .as_str()
            .map(|code| format!("  [{code}]"))
            .unwrap_or_default();
        let line = format!(
            "{}:{}:{}:{}:{}: {}: {}{}",
            row["file"].as_str().unwrap(),
            row["line"],
            row["column"],
            row["end_line"],
            row["end_column"],
            row["severity"].as_str().unwrap(),
            row["message"].as_str().unwrap(),
            code
        );
        assert_eq!(row["raw_line"], line);
        rendered.push_str(&line);
        rendered.push('\n');
    }
    assert_eq!(
        unhex(text_call["stdout_hex"].as_str().unwrap()),
        rendered.as_bytes()
    );
}

fn derive_projection(cell: &Value) -> Value {
    let texts = cell["original_text_diagnostics"].as_array().unwrap();
    let mut used = BTreeSet::new();
    let mut result = Vec::new();
    for row in cell["original_json_diagnostics"].as_array().unwrap() {
        let column = row["column"].as_u64().unwrap() + 1;
        let matched: Vec<_> = texts
            .iter()
            .enumerate()
            .filter(|(i, t)| {
                !used.contains(i)
                    && t["file"] == row["file"]
                    && t["line"] == row["line"]
                    && t["column"] == column
                    && t["end_line"] == row["end_line"]
                    && t["end_column"] == row["end_column"]
                    && t["severity"] == row["severity"]
                    && t["message"] == row["message"]
                    && if row["severity"] == "error" {
                        t["code"] == row["code"]
                    } else {
                        t["code"].is_null()
                    }
            })
            .collect();
        assert_eq!(matched.len(), 1);
        used.insert(matched[0].0);
        if let Some(hint) = row["hint"].as_str() {
            for note in hint.lines() {
                let matched: Vec<_> = texts
                    .iter()
                    .enumerate()
                    .filter(|(i, t)| {
                        !used.contains(i)
                            && t["file"] == row["file"]
                            && t["line"] == row["line"]
                            && t["column"] == column
                            && t["severity"] == "note"
                            && t["message"] == note
                            && t["code"].is_null()
                    })
                    .collect();
                assert_eq!(matched.len(), 1);
                used.insert(matched[0].0);
            }
        }
        result.push(
            json!({"rule":row["code"],"message":row["message"],"hint":row["hint"],
            "path":row["file"],"severity":row["severity"],"location":{"line":row["line"],
                "column":column,"end_line":row["end_line"],"end_column":row["end_column"]}}),
        );
    }
    assert_eq!(used.len(), texts.len());
    Value::Array(result)
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
            let digit = |byte| match byte {
                b'0'..=b'9' => byte - b'0',
                b'a'..=b'f' => byte - b'a' + 10,
                _ => panic!("invalid lowercase hex"),
            };
            digit(pair[0]) * 16 + digit(pair[1])
        })
        .collect()
}
