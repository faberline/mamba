//! Public file publication keeps the separately frozen private string API contract.
use super::super::*;
use crate::domain::{ports::ProjectStore, source::PythonVersion};
use crate::infrastructure::filesystem::Filesystem;
use serde_json::{json, Value};
use std::fs;

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[test]
fn pinned_public_cr_cimport_noop() {
    let fixture = include_str!("public_cr_stream.json");
    assert_eq!(
        format!("{:x}", Sha256::digest(fixture.as_bytes())),
        "e541c5fa79a96c004deb9dcd047cab9158661d8108285021d1cd712aaaecc11c"
    );
    let fixture: Value = serde_json::from_str(fixture).unwrap();
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("main.py");
    let input = b"\rimport a\r\rcimport z\r";
    fs::write(&path, input).unwrap();
    let profile = FixedProfile::locked();
    let mut workspace = Filesystem.load(std::slice::from_ref(&path), true);
    let mut rows = Vec::new();
    let mut failures = 0;
    for (index, version) in [
        PythonVersion::Py312,
        PythonVersion::Py313,
        PythonVersion::Py314,
    ]
    .into_iter()
    .enumerate()
    {
        workspace.projects[0].python_version = version;
        let expected = &fixture["cells"][index];
        assert_eq!(expected["target"], version.name());
        assert_eq!(hex(input), expected["source_bytes_hex"].as_str().unwrap());
        let disk_before = fs::read(&path).unwrap();
        let private = format(&workspace.sources[0], &workspace, &profile).unwrap();
        let checked = check(&workspace, &profile);
        let transformed = transform(&workspace, &profile);
        let output = match transformed.candidates.as_slice() {
            [] => Some(workspace.sources[0].text.clone()),
            [(source_index, text)] if *source_index == 0 => Some(text.clone()),
            _ => None,
        };
        let candidate_bytes = output.as_deref().map(|text| {
            Filesystem
                .candidate_bytes(&workspace.sources[0], text)
                .unwrap()
        });
        let repeated = transform(&workspace, &profile);
        let disk_after = fs::read(&path).unwrap();
        let matched = candidate_bytes.as_deref() == Some(input.as_slice())
            && transformed.candidates.is_empty()
            && transformed.issues.is_empty()
            && transformed.completion == CompletionStatus::Incomplete
            && json!(transformed.remaining) == expected["native_remaining"]
            && repeated.candidates.is_empty()
            && repeated.issues.is_empty()
            && repeated.completion == CompletionStatus::Incomplete
            && repeated.remaining == transformed.remaining
            && checked.completion == CompletionStatus::Incomplete
            && checked.reference_exit_code.is_none()
            && checked.checked_files == 1
            && checked.diagnostics.is_empty()
            && json!(checked.remaining) == expected["native_remaining"]
            && hex(private.0.as_bytes())
                == expected["private_code_api_output_hex"].as_str().unwrap()
            && private.1.is_empty()
            && disk_before == disk_after
            && disk_before.as_slice() == input.as_slice();
        failures += usize::from(!matched);
        rows.push(json!({"target":version.name(),"expected":expected,"matched":matched,
            "source_before_hex":hex(&disk_before),"source_after_hex":hex(&disk_after),
            "snapshot_bytes_hex":hex(&workspace.sources[0].bytes),"snapshot_sha256":workspace.sources[0].sha256,
            "private_api_output_hex":hex(private.0.as_bytes()),"private_remaining":private.1,
            "public_output_hex":candidate_bytes.as_deref().map(hex),
            "raw_check_tool_report":serde_json::to_value(&checked).unwrap(),
            "transform":{"completion":transformed.completion,"candidates":transformed.candidates,
                "issues":transformed.issues,"remaining":transformed.remaining},
            "repeat_transform":{"completion":repeated.completion,"candidates":repeated.candidates,
                "issues":repeated.issues,"remaining":repeated.remaining},
            "whole_acceptance":false}));
    }
    if let Some(destination) = std::env::var_os("MAMBA_QUALITY_ISORT_PUBLIC_CR_EVIDENCE") {
        fs::write(destination, serde_json::to_vec_pretty(&json!({"schema":"mamba.quality.isort-public-cr-comparison.v1",
            "fixture_sha256":"e541c5fa79a96c004deb9dcd047cab9158661d8108285021d1cd712aaaecc11c","cells":rows,"failed":failures,"whole_acceptance":false})).unwrap()).unwrap();
    }
    assert_eq!(
        failures, 0,
        "Public CR/cimport bytes must match the qualified file CLI"
    );
}
