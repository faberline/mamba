//! Exact nine-cell original union-member diagnostics. Keep original rows and exits intact.
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
    let bytes = include_bytes!("union_missing_members.json");
    assert_eq!(
        format!("{:x}", Sha256::digest(bytes)),
        "cf716eda6b27258bb1203e909e818a4b6bc8a730968bd537129ea942632b12dc"
    );
    let fixture: Value = serde_json::from_slice(bytes).unwrap();
    assert_eq!(fixture["cell_count"], 9);
    assert_eq!(fixture["source_count"], 9);
    assert_eq!(fixture["cells"].as_array().unwrap().len(), 9);
    assert_eq!(fixture["sources"].as_array().unwrap().len(), 9);
    let cell = &fixture["cells"][index];
    let temp = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(temp.path()).unwrap();
    for file in cell["files"].as_array().unwrap() {
        let bytes: Vec<u8> = file["bytes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| u8::try_from(v.as_u64().unwrap()).unwrap())
            .collect();
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
    let diagnostic_exit = i64::from(report.diagnostics.iter().any(|d| d.severity == "error"));
    assert_eq!(diagnostic_exit, cell["original_exit"].as_i64().unwrap());
    for file in cell["files"].as_array().unwrap() {
        assert_eq!(
            format!(
                "{:x}",
                Sha256::digest(fs::read(root.join(file["path"].as_str().unwrap())).unwrap())
            ),
            file["sha256"].as_str().unwrap()
        );
    }
}

#[test]
fn case_01_optional_none_append_py312() {
    check_cell(0);
}

#[test]
fn case_01_optional_none_append_py313() {
    check_cell(1);
}

#[test]
fn case_01_optional_none_append_py314() {
    check_cell(2);
}

#[test]
fn case_02_literal_and_int_missing_member_py312() {
    check_cell(3);
}

#[test]
fn case_02_literal_and_int_missing_member_py313() {
    check_cell(4);
}

#[test]
fn case_02_literal_and_int_missing_member_py314() {
    check_cell(5);
}

#[test]
fn case_03_list_and_str_missing_member_py312() {
    check_cell(6);
}

#[test]
fn case_03_list_and_str_missing_member_py313() {
    check_cell(7);
}

#[test]
fn case_03_list_and_str_missing_member_py314() {
    check_cell(8);
}
