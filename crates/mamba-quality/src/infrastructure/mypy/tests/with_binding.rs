//! The original Mypy contract keeps a variable introduced by a `with` target in function scope.
use crate::domain::{model::CompletionStatus, ports::ProjectStore, source::PythonVersion};
use crate::infrastructure::filesystem::Filesystem;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{fs, io::Write};

#[test]
fn pinned_with_target_integer_context_originals() {
    let bytes = include_bytes!("with_binding.json");
    assert_eq!(
        format!("{:x}", Sha256::digest(bytes)),
        "9fa4e86ee1495cd05e02426aa73fa06bf63cdb4eb4b9dcb6229ca6577a53403b"
    );
    let fixture: Value = serde_json::from_slice(bytes).unwrap();
    let source = fixture["source"].as_str().unwrap().as_bytes();
    assert_eq!(
        format!("{:x}", Sha256::digest(source)),
        "ed6be161845315ffd63b57231e28d87a27712c40c739435e3b775f4cfb04f1ea"
    );
    assert_eq!(
        source,
        &fixture["source_bytes_hex"]
            .as_str()
            .unwrap()
            .as_bytes()
            .chunks_exact(2)
            .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
            .collect::<Vec<_>>()[..]
    );
    assert_eq!(fixture["original_calls"].as_array().unwrap().len(), 12);
    assert_eq!(fixture["expected_diagnostics"], json!([]));

    let mut rows = Vec::new();
    let mut failed = 0usize;
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
        let actual_tool_report = json!(&report);
        let matched = actual_tool_report["diagnostics"] == fixture["expected_diagnostics"]
            && report.completion == CompletionStatus::Incomplete
            && report.reference_exit_code.is_none()
            && report.checked_files == 1
            && before == source
            && after == source;
        failed += usize::from(!matched);
        rows.push(json!({
            "target": target,
            "source_path": path,
            "source_before": before,
            "source_before_sha256": format!("{:x}", Sha256::digest(&before)),
            "source_after": after,
            "source_after_sha256": format!("{:x}", Sha256::digest(&after)),
            "actual_tool_report": actual_tool_report,
            "matched": matched
        }));
    }
    let evidence = json!({
        "schema": "mamba.quality.mypy.with-target-comparison.v1",
        "fixture_sha256": format!("{:x}", Sha256::digest(bytes)),
        "original_capture_manifest_sha256": fixture["original_capture_manifest_sha256"],
        "original_verification_sha256": fixture["original_verification_sha256"],
        "original_calls": fixture["original_calls"],
        "rows": rows,
        "failed": failed,
        "whole_acceptance": false
    });
    if let Some(destination) = std::env::var_os("MAMBA_QUALITY_MYPY_WITH_BINDING_EVIDENCE") {
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
        "with-target diagnostics differ from original Mypy"
    );
}
