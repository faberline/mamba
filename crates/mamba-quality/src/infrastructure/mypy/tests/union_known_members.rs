//! Exact nine-cell positive original union-known-member controls.
use crate::domain::{
    model::{CompletionStatus, QualityTool},
    ports::ProjectStore,
    source::PythonVersion,
};
use crate::infrastructure::filesystem::Filesystem;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs;

fn check_cell(index: usize) {
    let bytes = include_bytes!("union_known_members.json");
    assert_eq!(
        format!("{:x}", Sha256::digest(bytes)),
        "d9c426e7d057e13f7aac9ef8a3a5cd9a09fd38bf27333fb848f9b07887b31c95"
    );
    let fixture: Value = serde_json::from_slice(bytes).unwrap();
    assert_eq!(fixture["cell_count"], 9);
    assert_eq!(fixture["source_count"], 9);
    assert_eq!(fixture["cells"].as_array().unwrap().len(), 9);
    assert_eq!(fixture["sources"].as_array().unwrap().len(), 9);
    assert_eq!(fixture["whole_plan_accepted"], false);
    let cell = &fixture["cells"][index];
    assert_eq!(cell["original_exit"], 0);
    assert_eq!(cell["expected"], json!([]));
    assert_eq!(cell["original_diagnostics"], json!([]));
    assert_eq!(cell["original_stdout_bytes"], json!([10]));
    assert_eq!(cell["original_stderr_bytes"], json!([]));
    assert_eq!(cell["original_diagnostics_bytes"], json!([91, 93, 10]));
    let temp = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(temp.path()).unwrap();
    for file in cell["files"].as_array().unwrap() {
        let bytes: Vec<u8> = file["bytes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| u8::try_from(v.as_u64().unwrap()).unwrap())
            .collect();
        assert_eq!(bytes.len(), file["byte_count"].as_u64().unwrap() as usize);
        assert_eq!(
            format!("{:x}", Sha256::digest(&bytes)),
            file["sha256"].as_str().unwrap()
        );
        fs::write(root.join(file["path"].as_str().unwrap()), bytes).unwrap();
    }
    let inputs: Vec<_> = cell["inputs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| root.join(v.as_str().unwrap()))
        .collect();
    let mut workspace = Filesystem.load(&inputs, true);
    assert_eq!(workspace.projects.len(), 1);
    let target = match cell["target"].as_str().unwrap() {
        "3.12" => PythonVersion::Py312,
        "3.13" => PythonVersion::Py313,
        "3.14" => PythonVersion::Py314,
        other => panic!("Unexpected target {other}"),
    };
    workspace.projects[0].python_version = target;
    let report = super::super::check(&workspace);
    assert_eq!(report.tool, QualityTool::Mypy);
    assert_eq!(report.reference_version, "2.4.0");
    assert_eq!(report.completion, CompletionStatus::Incomplete);
    assert_eq!(report.reference_exit_code, None);
    assert_eq!(report.checked_files, inputs.len());
    let actual: Vec<_> = report
        .diagnostics
        .iter()
        .map(|d| {
            let raw = serde_json::to_value(d).unwrap();
            json!({"rule":raw["rule"],"message":d.message,"hint":d.hint,
            "path":d.path.strip_prefix(&root).unwrap(),"severity":d.severity,
            "location":{"line":d.location.line,"column":d.location.column,
                "end_line":d.location.end_line,"end_column":d.location.end_column}})
        })
        .collect();
    assert_eq!(
        Value::Array(actual),
        cell["expected"],
        "{} target {}",
        cell["case"],
        cell["target"]
    );
    assert_eq!(
        0,
        i64::from(report.diagnostics.iter().any(|d| d.severity == "error"))
    );
    for file in cell["files"].as_array().unwrap() {
        let bytes = fs::read(root.join(file["path"].as_str().unwrap())).unwrap();
        assert_eq!(
            format!("{:x}", Sha256::digest(bytes)),
            file["sha256"].as_str().unwrap()
        );
    }
}

#[test]
fn case_01_list_str_length_py312() {
    check_cell(0);
}
#[test]
fn case_01_list_str_length_py313() {
    check_cell(1);
}
#[test]
fn case_01_list_str_length_py314() {
    check_cell(2);
}
#[test]
fn case_02_literal_str_upper_py312() {
    check_cell(3);
}
#[test]
fn case_02_literal_str_upper_py313() {
    check_cell(4);
}
#[test]
fn case_02_literal_str_upper_py314() {
    check_cell(5);
}
#[test]
fn case_03_none_object_any_never_py312() {
    check_cell(6);
}
#[test]
fn case_03_none_object_any_never_py313() {
    check_cell(7);
}
#[test]
fn case_03_none_object_any_never_py314() {
    check_cell(8);
}
