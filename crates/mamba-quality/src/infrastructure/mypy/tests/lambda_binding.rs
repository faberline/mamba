//! Lambda parameters remain bound when result type inference is incomplete.
use crate::domain::{model::CompletionStatus, ports::ProjectStore, source::PythonVersion};
use crate::infrastructure::filesystem::Filesystem;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{fs, io::Write};

#[test]
fn pinned_lambda_parameter_binding_originals() {
    let bytes = include_bytes!("lambda_binding.json");
    assert_eq!(
        format!("{:x}", Sha256::digest(bytes)),
        "258adae7ca61e0d5bc6740c82d65c69073e0023e0306481e538a07cc02d8ee44"
    );
    let fixture: Value = serde_json::from_slice(bytes).unwrap();
    let source = fixture["source"].as_str().unwrap().as_bytes();
    assert_eq!(
        format!("{:x}", Sha256::digest(source)),
        fixture["source_sha256"]
    );
    assert_eq!(fixture["original_calls"].as_array().unwrap().len(), 12);
    assert_eq!(fixture["expected_diagnostics"], json!([]));
    let mut rows = Vec::new();
    let mut failed = 0;
    for (target, version) in [
        ("3.12", PythonVersion::Py312),
        ("3.13", PythonVersion::Py313),
        ("3.14", PythonVersion::Py314),
    ] {
        let root = tempfile::tempdir().unwrap();
        let root_path = fs::canonicalize(root.path()).unwrap();
        let path = root_path.join("main.py");
        fs::write(&path, source).unwrap();
        let before = fs::read(&path).unwrap();
        let mut workspace = Filesystem.load(std::slice::from_ref(&path), true);
        workspace.projects[0].python_version = version;
        let report = super::super::check(&workspace);
        let after = fs::read(&path).unwrap();
        let matched = report.diagnostics.is_empty()
            && report.completion == CompletionStatus::Incomplete
            && report.reference_exit_code.is_none()
            && report.checked_files == 1
            && before == source
            && after == source;
        failed += usize::from(!matched);
        rows.push(json!({"target":target,"source_path":path,"source_before":before,"source_after":after,"actual_tool_report":report,"matched":matched}));
    }
    let evidence = json!({"schema":"mamba.quality.mypy.lambda-binding-comparison.v1","fixture_sha256":format!("{:x}", Sha256::digest(bytes)),"original_capture_manifest_sha256":fixture["original_capture_manifest_sha256"],"original_verification_sha256":fixture["original_verification_sha256"],"rows":rows,"failed":failed,"whole_acceptance":false});
    if let Some(destination) = std::env::var_os("MAMBA_QUALITY_MYPY_LAMBDA_BINDING_EVIDENCE") {
        let mut output = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(destination)
            .unwrap();
        output
            .write_all(&serde_json::to_vec_pretty(&evidence).unwrap())
            .unwrap();
    }
    assert_eq!(rows.len(), 3);
    assert_eq!(
        failed, 0,
        "Lambda parameter diagnostics differ from original Mypy"
    );
}
