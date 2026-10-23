//! Locked fixed-profile checks and safe fix bytes retain original ordering.
use super::*;
use crate::domain::{ports::ProjectStore, source::PythonVersion};
use crate::infrastructure::filesystem::Filesystem;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fs, io::Write, path::PathBuf};
mod catalog;
mod interactions;
mod reports;

fn bytes(value: &Value) -> Vec<u8> {
    let text = value.as_str().unwrap();
    assert_eq!(text.len() % 2, 0);
    (0..text.len())
        .step_by(2)
        .map(|offset| u8::from_str_radix(&text[offset..offset + 2], 16).unwrap())
        .collect()
}

fn workspace(paths: &[PathBuf], version: PythonVersion) -> QualityWorkspace {
    let mut workspace = Filesystem.load(paths, true);
    for project in &mut workspace.projects {
        project.python_version = version;
    }
    workspace
}

fn expected(
    cases: &BTreeMap<String, Value>,
    metadata: &Value,
    target: usize,
    stage: &str,
) -> Value {
    let order = metadata["versions"][target][format!("{stage}_order")]
        .as_array()
        .unwrap();
    Value::Array(
        order
            .iter()
            .map(|position| {
                cases[position[0].as_str().unwrap()]["expected"][target][stage]
                    [position[1].as_u64().unwrap() as usize]
                    .clone()
            })
            .collect(),
    )
}

fn write_check_stage_status(
    writer: &mut impl Write,
    python: &str,
    stage: &str,
    report: &ToolReport,
) {
    #[derive(serde::Serialize)]
    struct Record<'a> {
        record: &'static str,
        python: &'a str,
        stage: &'a str,
        completion: &'a CompletionStatus,
        reference_exit_code: &'a Option<i32>,
        checked_files: usize,
        remaining: &'a [String],
    }
    serde_json::to_writer(
        &mut *writer,
        &Record {
            record: "stage",
            python,
            stage,
            completion: &report.completion,
            reference_exit_code: &report.reference_exit_code,
            checked_files: report.checked_files,
            remaining: &report.remaining,
        },
    )
    .expect("write Ruff check stage status");
    writer
        .write_all(b"\n")
        .expect("finish Ruff check stage status");
}

fn write_transform_stage_status(
    writer: &mut impl Write,
    python: &str,
    stage: &str,
    report: &TransformResult,
) {
    #[derive(serde::Serialize)]
    struct Record<'a> {
        record: &'static str,
        python: &'a str,
        stage: &'a str,
        completion: &'a CompletionStatus,
        remaining: &'a [String],
        issue_count: usize,
    }
    serde_json::to_writer(
        &mut *writer,
        &Record {
            record: "stage",
            python,
            stage,
            completion: &report.completion,
            remaining: &report.remaining,
            issue_count: report.issues.len(),
        },
    )
    .expect("write Ruff transform stage status");
    writer
        .write_all(b"\n")
        .expect("finish Ruff transform stage status");
}

#[test]
fn pinned_original_checks_and_safe_fixes() {
    let cases: BTreeMap<String, Value> = catalog::CHUNKS
        .iter()
        .flat_map(|chunk| serde_json::from_str::<Vec<Value>>(chunk).unwrap())
        .map(|case| (case["path"].as_str().unwrap().to_string(), case))
        .collect();
    assert_eq!(cases.len(), catalog::CASE_COUNT);
    let metadata: Value = serde_json::from_str(catalog::METADATA).unwrap();
    let profile = FixedProfile::locked();
    let root = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(root.path()).unwrap();
    fs::write(
        root.join("pyproject.toml"),
        "[project]\nrequires-python='>=3.12'\n",
    )
    .unwrap();
    let paths: Vec<_> = cases.keys().map(|path| root.join(path)).collect();
    let mut evidence = std::env::var_os("MAMBA_QUALITY_RUFF_EVIDENCE").map(|path| {
        let file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .expect("create new Ruff evidence file");
        let mut writer = std::io::BufWriter::new(file);
        writer
            .write_all(b"{\"schema\":\"mamba.quality.ruff-comparison.v1\",\"corpus_sha256\":")
            .expect("write Ruff evidence header");
        serde_json::to_writer(&mut writer, catalog::CORPUS_SHA256).expect("write Ruff corpus hash");
        writer
            .write_all(b",\"versions\":[")
            .expect("write Ruff versions header");
        writer
    });
    let mut fixed_bytes_evidence =
        std::env::var_os("MAMBA_QUALITY_RUFF_FIXED_BYTES_EVIDENCE").map(|path| {
            let file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path)
                .expect("create new Ruff fixed bytes evidence file");
            let mut writer = std::io::BufWriter::new(file);
            writer
                .write_all(b"{\"record\":\"header\",\"schema\":\"mamba.quality.ruff-fixed-output-bytes.v1\",\"corpus_sha256\":")
                .expect("write Ruff fixed bytes header");
            serde_json::to_writer(&mut writer, catalog::CORPUS_SHA256)
                .expect("write Ruff fixed bytes corpus hash");
            writer
                .write_all(b"}\n")
                .expect("finish Ruff fixed bytes header");
            writer
        });
    let mut fixed_byte_records_written = 0usize;
    let mut stage_status_evidence =
        std::env::var_os("MAMBA_QUALITY_RUFF_STAGE_STATUS_EVIDENCE").map(|path| {
            let file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path)
                .expect("create new Ruff stage status evidence file");
            let mut writer = std::io::BufWriter::new(file);
            writer
                .write_all(b"{\"record\":\"header\",\"schema\":\"mamba.quality.ruff-stage-status.v1\",\"corpus_sha256\":")
                .expect("write Ruff stage status header");
            serde_json::to_writer(&mut writer, catalog::CORPUS_SHA256)
                .expect("write Ruff stage status corpus hash");
            writer
                .write_all(b"}\n")
                .expect("finish Ruff stage status header");
            writer
        });
    let mut stage_records_written = 0usize;
    let mut first_version = true;
    let mut versions_seen = 0usize;
    let mut file_byte_comparisons = 0usize;
    let mut failures = 0;
    for (target, version) in [
        PythonVersion::Py312,
        PythonVersion::Py313,
        PythonVersion::Py314,
    ]
    .into_iter()
    .enumerate()
    {
        for (name, case) in &cases {
            let path = root.join(name);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            let raw = bytes(&case["source_hex"]);
            assert_eq!(
                format!("{:x}", Sha256::digest(&raw)),
                case["source_sha256"].as_str().unwrap()
            );
            fs::write(path, raw).unwrap();
        }
        let clean_paths: Vec<_> = metadata["versions"][target]["clean_paths"]
            .as_array()
            .unwrap()
            .iter()
            .map(|name| root.join(name.as_str().unwrap()))
            .collect();
        let clean = check(&workspace(&clean_paths, version), &profile);
        let clean_actual = reports::diagnostics(&clean, &root);
        let clean_equal = clean_actual == metadata["versions"][target]["clean_expected"]
            && clean.reference_exit_code == Some(0)
            && clean.completion == CompletionStatus::Complete;
        failures += usize::from(!clean_equal);
        let clean_exit = clean.reference_exit_code;
        if let Some(writer) = stage_status_evidence.as_mut() {
            write_check_stage_status(writer, version.name(), "clean", &clean);
            stage_records_written += 1;
        }
        drop(clean);
        let original = workspace(&paths, version);
        let before = check(&original, &profile);
        let before_expected = expected(&cases, &metadata, target, "before");
        let before_actual = reports::diagnostics(&before, &root);
        let before_equal = before_actual == before_expected
            && before.reference_exit_code
                == metadata["versions"][target]["before_exit"]
                    .as_u64()
                    .map(|code| code as i32)
            && before.completion == CompletionStatus::Complete;
        failures += usize::from(!before_equal);
        let before_exit = before.reference_exit_code;
        if let Some(writer) = stage_status_evidence.as_mut() {
            write_check_stage_status(writer, version.name(), "before", &before);
            stage_records_written += 1;
        }
        drop(before);
        let fixed = transform(&original, &profile);
        if let Some(writer) = stage_status_evidence.as_mut() {
            write_transform_stage_status(writer, version.name(), "first_transform", &fixed);
            stage_records_written += 1;
        }
        let mut candidate_bytes: Vec<_> = original
            .sources
            .iter()
            .map(|source| source.bytes.clone())
            .collect();
        for (index, candidate) in &fixed.candidates {
            candidate_bytes[*index] = Filesystem
                .candidate_bytes(&original.sources[*index], candidate)
                .unwrap();
        }
        let mut file_results = Vec::new();
        for (source, output) in original.sources.iter().zip(&candidate_bytes) {
            let name = source.path.strip_prefix(&root).unwrap().to_str().unwrap();
            let expected = bytes(&cases[name]["expected"][target]["output_hex"]);
            let equal = *output == expected;
            failures += usize::from(!equal);
            file_byte_comparisons += 1;
            if let Some(writer) = fixed_bytes_evidence.as_mut() {
                writer
                    .write_all(b"{\"record\":\"file\",\"python\":")
                    .expect("write Ruff fixed bytes file header");
                serde_json::to_writer(&mut *writer, version.name())
                    .expect("write Ruff fixed bytes target");
                writer
                    .write_all(b",\"path\":")
                    .expect("write Ruff fixed bytes path key");
                serde_json::to_writer(&mut *writer, name).expect("write Ruff fixed bytes path");
                write!(
                    writer,
                    ",\"actual_byte_count\":{},\"actual_output_hex\":\"",
                    output.len()
                )
                .expect("write Ruff fixed bytes length");
                const HEX: &[u8; 16] = b"0123456789abcdef";
                let mut encoded = [0u8; 4096];
                for chunk in output.chunks(encoded.len() / 2) {
                    for (index, &byte) in chunk.iter().enumerate() {
                        encoded[index * 2] = HEX[usize::from(byte >> 4)];
                        encoded[index * 2 + 1] = HEX[usize::from(byte & 15)];
                    }
                    writer
                        .write_all(&encoded[..chunk.len() * 2])
                        .expect("write Ruff fixed bytes hex chunk");
                }
                writer
                    .write_all(b"\"}\n")
                    .expect("finish Ruff fixed bytes file record");
                fixed_byte_records_written += 1;
            }
            file_results.push(json!({"path":name,"equal":equal,
                "expected_sha256":cases[name]["expected"][target]["output_sha256"],
                "actual_sha256":format!("{:x}",Sha256::digest(output)),
                "changed":*output != source.bytes}));
            fs::write(&source.path, output).unwrap();
        }
        let errors = reports::errors(&fixed, &original, &root, &cases, target);
        failures += errors
            .iter()
            .filter(|issue| issue["expected"].as_bool() != Some(true))
            .count();
        drop(fixed);
        drop(candidate_bytes);
        drop(original);
        let after_workspace = workspace(&paths, version);
        let after = check(&after_workspace, &profile);
        let after_expected = expected(&cases, &metadata, target, "after");
        let after_actual = reports::diagnostics(&after, &root);
        let after_equal = after_actual == after_expected
            && after.reference_exit_code
                == metadata["versions"][target]["fix_exit"]
                    .as_u64()
                    .map(|code| code as i32)
            && after.completion == CompletionStatus::Complete;
        failures += usize::from(!after_equal);
        let after_exit = after.reference_exit_code;
        if let Some(writer) = stage_status_evidence.as_mut() {
            write_check_stage_status(writer, version.name(), "after", &after);
            stage_records_written += 1;
        }
        drop(after);
        let repeated = transform(&after_workspace, &profile);
        if let Some(writer) = stage_status_evidence.as_mut() {
            write_transform_stage_status(writer, version.name(), "repeated_transform", &repeated);
            stage_records_written += 1;
        }
        let stable = repeated.candidates.is_empty();
        failures += usize::from(!stable);
        let repeat_errors = reports::errors(&repeated, &after_workspace, &root, &cases, target);
        failures += repeat_errors
            .iter()
            .filter(|issue| issue["expected"].as_bool() != Some(true))
            .count();
        drop(repeated);
        drop(after_workspace);
        versions_seen += 1;
        if let Some(writer) = evidence.as_mut() {
            if !first_version {
                writer
                    .write_all(b",")
                    .expect("write Ruff version separator");
            }
            serde_json::to_writer(
                &mut *writer,
                &json!({"python":version.name(), "files":file_results,
                    "clean_equal":clean_equal,"clean_actual":clean_actual,"clean_exit":clean_exit,
                    "before_equal":before_equal,"after_equal":after_equal,"stable":stable,
                    "before_expected":before_expected,"before_actual":before_actual,
                    "after_expected":after_expected,"after_actual":after_actual,
                    "before_exit":before_exit,"after_exit":after_exit,
                    "fix_errors":errors,"repeat_errors":repeat_errors,
                    "matrix":metadata["versions"][target]["matrix"]}),
            )
            .expect("write Ruff version evidence");
            first_version = false;
        }
    }
    assert_eq!(versions_seen, 3);
    assert_eq!(file_byte_comparisons, catalog::CASE_COUNT * versions_seen);
    assert_eq!(file_byte_comparisons, 2352);
    if let Some(mut writer) = stage_status_evidence {
        assert_eq!(stage_records_written, versions_seen * 5);
        assert_eq!(stage_records_written, 15);
        serde_json::to_writer(
            &mut writer,
            &json!({"record":"footer","versions_seen":versions_seen,
                "stage_records_written":stage_records_written}),
        )
        .expect("write Ruff stage status footer");
        writer
            .write_all(b"\n")
            .expect("finish Ruff stage status footer");
        writer
            .flush()
            .expect("flush complete Ruff stage status evidence");
    }
    if let Some(mut writer) = fixed_bytes_evidence {
        assert_eq!(fixed_byte_records_written, file_byte_comparisons);
        assert_eq!(fixed_byte_records_written, 2352);
        serde_json::to_writer(
            &mut writer,
            &json!({"record":"footer","versions_seen":versions_seen,
                "file_byte_comparisons":file_byte_comparisons,
                "file_records_written":fixed_byte_records_written}),
        )
        .expect("write Ruff fixed bytes footer");
        writer
            .write_all(b"\n")
            .expect("finish Ruff fixed bytes footer");
        writer
            .flush()
            .expect("flush complete Ruff fixed bytes evidence");
    }
    if let Some(mut writer) = evidence {
        write!(
            writer,
            "],\"failed\":{failures},\"original_dormant_rules\":[\"F621\"],\"whole_suite_accepted\":false}}"
        )
        .expect("write Ruff evidence footer");
        writer.flush().expect("flush complete Ruff evidence");
    }
    assert_eq!(failures, 0, "Pinned Ruff checks or safe fix bytes failed");
}
