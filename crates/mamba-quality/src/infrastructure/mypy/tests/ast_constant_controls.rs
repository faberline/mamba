//! Exact 36-cell AST Constant originals. No expected diagnostic is inferred.
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
    let bytes = include_bytes!("ast_constant_controls.json");
    assert_eq!(
        format!("{:x}", Sha256::digest(bytes)),
        "08e3a2d93ebbe4989c58eab4969b2b76ea82d6f93615e59de4f56721415309f6"
    );
    let fixture: Value = serde_json::from_slice(bytes).unwrap();
    assert_eq!(fixture["cell_count"], 36);
    assert_eq!(fixture["source_count"], 39);
    assert_eq!(fixture["cells"].as_array().unwrap().len(), 36);
    assert_eq!(fixture["sources"].as_array().unwrap().len(), 39);
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
fn case_01_wrong_constructor_value_list_py312() {
    check_cell(0);
}

#[test]
fn case_01_wrong_constructor_value_list_py313() {
    check_cell(1);
}

#[test]
fn case_01_wrong_constructor_value_list_py314() {
    check_cell(2);
}

#[test]
fn case_02_wrong_constructor_kind_int_py312() {
    check_cell(3);
}

#[test]
fn case_02_wrong_constructor_kind_int_py313() {
    check_cell(4);
}

#[test]
fn case_02_wrong_constructor_kind_int_py314() {
    check_cell(5);
}

#[test]
fn case_03_replace_positional_value_py312() {
    check_cell(6);
}

#[test]
fn case_03_replace_positional_value_py313() {
    check_cell(7);
}

#[test]
fn case_03_replace_positional_value_py314() {
    check_cell(8);
}

#[test]
fn case_04_replace_unknown_key_py312() {
    check_cell(9);
}

#[test]
fn case_04_replace_unknown_key_py313() {
    check_cell(10);
}

#[test]
fn case_04_replace_unknown_key_py314() {
    check_cell(11);
}

#[test]
fn case_05_replace_wrong_value_type_py312() {
    check_cell(12);
}

#[test]
fn case_05_replace_wrong_value_type_py313() {
    check_cell(13);
}

#[test]
fn case_05_replace_wrong_value_type_py314() {
    check_cell(14);
}

#[test]
fn case_06_replace_wrong_kind_type_py312() {
    check_cell(15);
}

#[test]
fn case_06_replace_wrong_kind_type_py313() {
    check_cell(16);
}

#[test]
fn case_06_replace_wrong_kind_type_py314() {
    check_cell(17);
}

#[test]
fn case_07_replace_version_availability_py312() {
    check_cell(18);
}

#[test]
fn case_07_replace_version_availability_py313() {
    check_cell(19);
}

#[test]
fn case_07_replace_version_availability_py314() {
    check_cell(20);
}

#[test]
fn case_08_legacy_n_s_writes_py312() {
    check_cell(21);
}

#[test]
fn case_08_legacy_n_s_writes_py313() {
    check_cell(22);
}

#[test]
fn case_08_legacy_n_s_writes_py314() {
    check_cell(23);
}

#[test]
fn case_09_module_alias_formatting_py312() {
    check_cell(24);
}

#[test]
fn case_09_module_alias_formatting_py313() {
    check_cell(25);
}

#[test]
fn case_09_module_alias_formatting_py314() {
    check_cell(26);
}

#[test]
fn case_10_direct_import_alias_formatting_py312() {
    check_cell(27);
}

#[test]
fn case_10_direct_import_alias_formatting_py313() {
    check_cell(28);
}

#[test]
fn case_10_direct_import_alias_formatting_py314() {
    check_cell(29);
}

#[test]
fn case_11_direct_import_local_ast_shadow_py312() {
    check_cell(30);
}

#[test]
fn case_11_direct_import_local_ast_shadow_py313() {
    check_cell(31);
}

#[test]
fn case_11_direct_import_local_ast_shadow_py314() {
    check_cell(32);
}

#[test]
fn case_12_value_narrowing_formatting_py312() {
    check_cell(33);
}

#[test]
fn case_12_value_narrowing_formatting_py313() {
    check_cell(34);
}

#[test]
fn case_12_value_narrowing_formatting_py314() {
    check_cell(35);
}
