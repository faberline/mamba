//! Original Black byte comparisons. No incomplete engine is admitted by this gate.
use super::super::*;
use crate::domain::{ports::QualityEngine, source::PythonVersion};
use crate::{domain::ports::ProjectStore, infrastructure::filesystem::Filesystem};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{fs, io::Write, path::Path};

fn error_log_artifact(message: &str) -> Option<Value> {
    let path = [
        "This diff might be helpful: ",
        "This invalid output might be helpful: ",
    ]
    .into_iter()
    .find_map(|marker| message.split_once(marker).map(|(_, path)| path.trim()))
    .filter(|path| {
        Path::new(path)
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with("blk_") && name.ends_with(".log"))
    })?;
    match fs::read(path) {
        Ok(bytes) => Some(json!({
            "path": path,
            "bytes_hex": bytes.iter().map(|byte| format!("{byte:02x}")).collect::<String>(),
            "sha256": format!("{:x}", Sha256::digest(&bytes)),
            "read_error": null,
        })),
        Err(error) => Some(json!({
            "path": path,
            "bytes_hex": null,
            "sha256": null,
            "read_error": error.to_string(),
        })),
    }
}

#[test]
fn pinned_original_corpus() {
    let mut evidence = std::env::var_os("MAMBA_QUALITY_BLACK_EVIDENCE").map(|destination| {
        let file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(destination)
            .expect("create new Black evidence file");
        let mut writer = std::io::BufWriter::new(file);
        writer
            .write_all(b"{\"cases\":[")
            .expect("write Black evidence header");
        writer
    });
    let root = tempfile::tempdir().unwrap();
    let mut first_result = true;
    let mut cases_seen = 0usize;
    let mut comparisons = 0usize;
    let mut failures = 0;
    for (index, case) in super::catalog::CHUNKS
        .iter()
        .flat_map(|chunk| serde_json::from_str::<Vec<Value>>(chunk).unwrap())
        .enumerate()
    {
        cases_seen += 1;
        let path = root.path().join(format!(
            "case_{index}.{}",
            case["extension"].as_str().unwrap_or("py")
        ));
        let source = case["source"].as_str().unwrap();
        fs::write(&path, source).unwrap();
        let mut workspace = Filesystem.load(std::slice::from_ref(&path), true);
        assert_eq!(
            format!("{:x}", Sha256::digest(source.as_bytes())),
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
            if let Some(destination) = std::env::var_os("MAMBA_QUALITY_BLACK_PROGRESS") {
                fs::write(destination, serde_json::to_vec_pretty(&json!({
                    "id":case["id"],"origins":case["origins"],"python":version.name(),"source":source
                })).unwrap()).unwrap();
            }
            workspace.projects[0].python_version = version;
            workspace.sources[0].text = source.into();
            let expected = &case["expected"][target];
            let (output, error, error_class) = match format(&workspace.sources[0], &workspace) {
                Ok(output) => (Some(output), None, None),
                Err(error) => (None, Some(error.message), Some(error.code)),
            };
            let layout_candidate = if error_class.as_deref() == Some("black.parsing.ASTSafetyError")
            {
                layout::format(
                    &workspace.sources[0],
                    super::super::super::native::target(&workspace, &workspace.sources[0]),
                )
                .ok()
            } else {
                None
            };
            let equal = if let Some(reference) = expected["reference_error"].as_str() {
                reference.split_once(": ").is_some_and(|(kind, message)| {
                    error_class.as_deref() == Some(format!("black.parsing.{kind}").as_str())
                        && error.as_deref().is_some_and(|error| {
                            if kind == "ASTSafetyError" {
                                super::safety::message_equal(error, message)
                            } else {
                                error == message
                            }
                        })
                })
            } else {
                output.as_deref() == expected["output"].as_str()
            };
            let public_report = check(&workspace);
            let public_error_logs = public_report
                .diagnostics
                .iter()
                .filter_map(|diagnostic| error_log_artifact(&diagnostic.message))
                .collect::<Vec<_>>();
            let public_tool_report = serde_json::to_value(public_report).unwrap();
            let mut stable = true;
            let mut equivalent = None;
            let mut ast_validation = None;
            let mut repeated = None;
            let mut repeat_error = None;
            let mut repeat_error_class = None;
            if let Some(output) = &output {
                let validation = super::super::super::native::NativeEngine.validate(
                    &workspace.sources[0],
                    output,
                    &workspace,
                    true,
                );
                equivalent = Some(validation.is_ok());
                ast_validation = Some(match validation {
                    Ok(()) => json!({"status": "valid", "issue": null}),
                    Err(issue) => json!({"status": "error", "issue": issue}),
                });
                workspace.sources[0].text = output.clone();
                match format(&workspace.sources[0], &workspace) {
                    Ok(second) => {
                        stable = second == *output;
                        repeated = Some(second);
                    }
                    Err(error) => {
                        stable = false;
                        repeat_error_class = Some(error.code);
                        repeat_error = Some(error.message);
                    }
                }
            }
            let changed_structure_unsafe =
                equivalent == Some(false) && output.as_deref() != Some(source);
            if !equal || !stable || changed_structure_unsafe {
                failures += 1;
            }
            let mut error_log_artifacts = error
                .as_deref()
                .and_then(error_log_artifact)
                .into_iter()
                .collect::<Vec<_>>();
            if let Some(artifact) = repeat_error.as_deref().and_then(error_log_artifact) {
                error_log_artifacts.push(artifact);
            }
            error_log_artifacts.extend(public_error_logs);
            comparisons += 1;
            if let Some(writer) = evidence.as_mut() {
                if !first_result {
                    writer
                        .write_all(b",")
                        .expect("write Black evidence separator");
                }
                serde_json::to_writer(
                    &mut *writer,
                    &json!({"id": case["id"], "python": version.name(),
                "source": source, "origins": case["origins"], "expected": expected,
                "frozen_reference_status": {
                    "source": "frozen_catalog_computed_status_not_process_exit",
                    "reference_check_exit": expected["reference_check_exit"],
                    "reference_converges": expected["reference_converges"],
                    "process_exit_observed_in_this_test": false,
                },
                "actual": output, "error": error, "error_class": error_class,
                "equal": equal, "stable": stable,
                "structure_equivalent": equivalent, "ast_validation": ast_validation,
                "repeated": repeated,
                "changed_structure_unsafe": changed_structure_unsafe,
                "repeat_error": repeat_error, "repeat_error_class": repeat_error_class,
                "public_tool_report": public_tool_report,
                "error_log_artifacts": error_log_artifacts,
                "layout_candidate": layout_candidate}),
                )
                .expect("write Black evidence row");
                first_result = false;
            }
        }
    }
    assert_eq!(cases_seen, super::catalog::CASE_COUNT);
    assert_eq!(comparisons, super::catalog::CASE_COUNT * 3);
    if let Some(writer) = evidence.as_mut() {
        writer
            .write_all(b"],\"corpus_sha256\":")
            .expect("write Black evidence footer");
        serde_json::to_writer(&mut *writer, super::catalog::CORPUS_SHA256)
            .expect("write Black corpus hash");
        let equal = comparisons - failures;
        write!(writer,
            ",\"equal\":{equal},\"exception_message_parity\":true,\"failed\":{failures},\"schema\":\"mamba.quality.black-comparison.v1\",\"whole_suite_accepted\":false}}")
            .expect("write Black evidence completion");
        writer.flush().expect("flush complete Black evidence");
    }
    assert_eq!(failures, 0, "Pinned Black byte comparisons failed");
}
