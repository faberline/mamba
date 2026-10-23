//! Frozen original Black raw-byte controls.
use super::super::*;
use crate::domain::{
    ports::{ProjectStore, QualityEngine},
    source::PythonVersion,
};
use crate::infrastructure::{filesystem::Filesystem, native::NativeEngine};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs;

const FIXTURE_SHA256: &str = "e606bca84ab7fc3778dd21f19b78ed1d12e1c194bd5fa87b490886518ade8a9d";

fn unhex(input: &str) -> Result<Vec<u8>, String> {
    if !input.len().is_multiple_of(2) {
        return Err(format!("odd hex length {}", input.len()));
    }
    (0..input.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&input[i..i + 2], 16).map_err(|e| e.to_string()))
        .collect()
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn to_hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(char::from(DIGITS[(byte >> 4) as usize]));
        encoded.push(char::from(DIGITS[(byte & 0x0f) as usize]));
    }
    encoded
}

fn field(rows: &mut Vec<Value>, name: &str, expected: Value, actual: Value) -> bool {
    let passed = expected == actual;
    rows.push(json!({"field":name,"expected":expected,"actual":actual,"passed":passed}));
    passed
}

fn version(target: &str) -> Option<PythonVersion> {
    match target {
        "3.12" => Some(PythonVersion::Py312),
        "3.13" => Some(PythonVersion::Py313),
        "3.14" => Some(PythonVersion::Py314),
        _ => None,
    }
}

fn save_evidence(corpus: &str, rows: &[Value]) {
    let Some(path) = std::env::var_os("MAMBA_QUALITY_BLACK_BYTE_CONTROLS_EVIDENCE") else {
        return;
    };
    let payload = json!({
        "schema":"mamba.quality.black-byte-controls-comparison.v1",
        "fixture_sha256":sha256(corpus.as_bytes()), "comparisons":rows,
        "cell_count":rows.len(),
        "passed_cells":rows.iter().filter(|r| r["passed"] == true).count(),
        "failed_cells":rows.iter().filter(|r| r["passed"] == false).count(),
        "whole_suite_accepted":false
    });
    fs::write(
        path,
        serde_json::to_vec_pretty(&payload).expect("serialize evidence"),
    )
    .expect("write evidence");
}

#[test]
fn preserves_original_bytes_and_stable_black_candidates() {
    let corpus = include_str!("byte_controls.json");
    let fixture: Value = serde_json::from_str(corpus).expect("parse frozen fixture");
    assert_eq!(sha256(corpus.as_bytes()), FIXTURE_SHA256);
    let cells = fixture["cells"].as_array().expect("fixture cells");
    assert_eq!(cells.len(), 36);

    let temp = tempfile::tempdir().expect("temporary root");
    let root = fs::canonicalize(temp.path()).expect("canonical temporary root");
    let mut comparisons = Vec::with_capacity(36);
    let mut failed = 0usize;

    for (index, cell) in cells.iter().enumerate() {
        let name = cell["name"].as_str().unwrap_or("<missing-name>");
        let target = cell["python_target"].as_str().unwrap_or("<missing-target>");
        let scope = cell["scope"].as_str().unwrap_or("<missing-scope>");
        let path = root.join(format!("cell_{index}_{name}.py"));
        let mut fields = Vec::new();
        let mut passed = true;
        let source_hex = cell["source_hex"].as_str().unwrap_or("");
        let source = match unhex(source_hex) {
            Ok(bytes) => bytes,
            Err(error) => {
                field(&mut fields, "source_hex_decodes", json!(true), json!(error));
                comparisons.push(json!({"name":name,"python_target":target,"scope":scope,"fields":fields,"passed":false}));
                failed += 1;
                continue;
            }
        };
        passed &= field(
            &mut fields,
            "source_sha256",
            cell["source_sha256"].clone(),
            json!(sha256(&source)),
        );
        fs::write(&path, &source).expect("write exact frozen source bytes");
        let mut workspace = Filesystem.load(std::slice::from_ref(&path), true);
        passed &= field(
            &mut fields,
            "loaded_source_count",
            json!(1),
            json!(workspace.sources.len()),
        );
        passed &= field(
            &mut fields,
            "loaded_source_bytes_hex",
            json!(source_hex),
            json!(workspace.sources.first().map(|s| to_hex(&s.bytes))),
        );

        if scope == "source-loader-only" {
            let issue = workspace
                .issues
                .iter()
                .find(|i| i.code == "source-encoding");
            passed &= field(
                &mut fields,
                "loader_issue_code",
                cell["loader_expected"]["issue_code"].clone(),
                json!(issue.map(|i| i.code.clone())),
            );
            passed &= field(
                &mut fields,
                "loader_issue_message",
                cell["loader_expected"]["message"].clone(),
                json!(issue.map(|i| i.message.clone())),
            );
            passed &= field(
                &mut fields,
                "loader_issue_path",
                json!(path.display().to_string()),
                json!(issue
                    .and_then(|i| i.path.as_ref())
                    .map(|p| p.display().to_string())),
            );
            // The frozen CLI exit is provenance only. Do not call Black format for this row.
            passed &= field(
                &mut fields,
                "original_black_cli_exit_provenance",
                json!(123),
                cell["original_black_cli_exit"].clone(),
            );
        } else {
            let Some(py_version) = version(target) else {
                field(
                    &mut fields,
                    "supported_python_target",
                    json!(true),
                    json!(target),
                );
                comparisons.push(json!({"name":name,"python_target":target,"scope":scope,"fields":fields,"passed":false}));
                failed += 1;
                continue;
            };
            if let Some(project) = workspace.projects.first_mut() {
                project.python_version = py_version;
            }
            let Some(source_snapshot) = workspace.sources.first().cloned() else {
                field(
                    &mut fields,
                    "loaded_source_available",
                    json!(true),
                    json!(false),
                );
                comparisons.push(json!({"name":name,"python_target":target,"scope":scope,"fields":fields,"passed":false}));
                failed += 1;
                continue;
            };
            let stable_hex = cell["stable_hex"].as_str().unwrap_or("");
            let expected_bytes = match unhex(stable_hex) {
                Ok(bytes) => bytes,
                Err(error) => {
                    field(&mut fields, "stable_hex_decodes", json!(true), json!(error));
                    comparisons.push(json!({"name":name,"python_target":target,"scope":scope,"fields":fields,"passed":false}));
                    failed += 1;
                    continue;
                }
            };
            passed &= field(
                &mut fields,
                "stable_sha256",
                cell["stable_sha256"].clone(),
                json!(sha256(&expected_bytes)),
            );
            let output = match format(&source_snapshot, &workspace) {
                Ok(text) => text,
                Err(issue) => {
                    field(
                        &mut fields,
                        "format_result",
                        json!("Ok"),
                        json!({"Err":{"code":issue.code,"message":issue.message}}),
                    );
                    comparisons.push(json!({"name":name,"python_target":target,"scope":scope,"fields":fields,"passed":false}));
                    failed += 1;
                    continue;
                }
            };
            let candidate = match Filesystem.candidate_bytes(&source_snapshot, &output) {
                Ok(bytes) => bytes,
                Err(issue) => {
                    field(
                        &mut fields,
                        "candidate_encoding",
                        json!("Ok"),
                        json!({"Err":{"code":issue.code,"message":issue.message}}),
                    );
                    comparisons.push(json!({"name":name,"python_target":target,"scope":scope,"fields":fields,"passed":false}));
                    failed += 1;
                    continue;
                }
            };
            passed &= field(
                &mut fields,
                "stable_candidate_bytes_hex",
                json!(stable_hex),
                json!(to_hex(&candidate)),
            );
            passed &= field(
                &mut fields,
                "stable_candidate_sha256",
                cell["stable_sha256"].clone(),
                json!(sha256(&candidate)),
            );

            let ast = NativeEngine.validate(&source_snapshot, &output, &workspace, true);
            passed &= field(
                &mut fields,
                "ast_validation",
                json!("Ok"),
                json!(if ast.is_ok() { "Ok" } else { "Err" }),
            );
            if let Err(error) = ast {
                fields.push(json!({"field":"ast_validation_error","actual":error}));
            }

            let mut second_workspace = workspace.clone();
            second_workspace.sources[0].text = output.clone();
            match format(&second_workspace.sources[0], &second_workspace) {
                Ok(second_text) => {
                    passed &= field(
                        &mut fields,
                        "repeat_format_text",
                        json!(output),
                        json!(second_text),
                    );
                    match Filesystem.candidate_bytes(&source_snapshot, &second_text) {
                        Ok(bytes) => {
                            passed &= field(
                                &mut fields,
                                "repeat_candidate_bytes_hex",
                                json!(to_hex(&candidate)),
                                json!(to_hex(&bytes)),
                            )
                        }
                        Err(issue) => {
                            passed &= field(
                                &mut fields,
                                "repeat_candidate_encoding",
                                json!("Ok"),
                                json!({"Err":{"code":issue.code,"message":issue.message}}),
                            )
                        }
                    }
                }
                Err(issue) => {
                    passed &= field(
                        &mut fields,
                        "repeat_format",
                        json!("Ok"),
                        json!({"Err":{"code":issue.code,"message":issue.message}}),
                    )
                }
            }

            let public = check(&workspace);
            passed &= field(
                &mut fields,
                "public_completion",
                json!(CompletionStatus::Incomplete),
                json!(public.completion),
            );
            passed &= field(
                &mut fields,
                "public_reference_exit_code",
                json!(null),
                json!(public.reference_exit_code),
            );
            passed &= field(
                &mut fields,
                "public_checked_files",
                json!(1),
                json!(public.checked_files),
            );
            let should_have_format_diagnostic = output != source_snapshot.text;
            let original_reference_exit = cell["original_initial_check_exit"].as_i64();
            passed &= field(
                &mut fields,
                "reference_check_exit_matches_candidate_change",
                json!(original_reference_exit.map(|exit| exit == 1)),
                json!(Some(should_have_format_diagnostic)),
            );
            let has_format_diagnostic = public.diagnostics.iter().any(|d| {
                d.tool == QualityTool::Black
                    && d.rule == "format"
                    && d.message == format!("would reformat {}", path.display())
                    && d.path == path
            });
            passed &= field(
                &mut fields,
                "public_format_diagnostic_present",
                json!(should_have_format_diagnostic),
                json!(has_format_diagnostic),
            );
            passed &= field(
                &mut fields,
                "public_nonformat_diagnostics_absent",
                json!(true),
                json!(!public.diagnostics.iter().any(|d| d.rule != "format")),
            );
        }

        let disk_bytes = fs::read(&path).expect("read source after non-publishing checks");
        passed &= field(
            &mut fields,
            "original_disk_bytes_hex",
            json!(source_hex),
            json!(to_hex(&disk_bytes)),
        );
        comparisons.push(json!({"name":name,"python_target":target,"scope":scope,"fields":fields,"passed":passed}));
        failed += usize::from(!passed);
    }

    save_evidence(corpus, &comparisons);
    assert_eq!(
        comparisons.len(),
        36,
        "record each frozen source/target cell"
    );
    assert_eq!(
        failed, 0,
        "frozen original byte controls differ; inspect the evidence JSON"
    );
}
