//! Pinned original slot probes test both build failures and successful builds.
use crate::domain::{model::CompletionStatus, ports::ProjectStore, source::PythonVersion};
use crate::infrastructure::filesystem::Filesystem;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs;

#[test]
fn pinned_slot_build_probes() {
    let cases: Vec<Value> = serde_json::from_str(include_str!("build_probes.json")).unwrap();
    assert_eq!(cases.len(), 660);
    let mut results = Vec::new();
    let mut failures = 0;
    for case in cases {
        let root = tempfile::tempdir().unwrap();
        let root_path = fs::canonicalize(root.path()).unwrap();
        let name = case["filename"].as_str().unwrap();
        let source = case["source"].as_str().unwrap();
        assert_eq!(
            format!("{:x}", Sha256::digest(source.as_bytes())),
            case["source_sha256"]
        );
        fs::write(root_path.join(name), source).unwrap();
        fs::write(
            root_path.join("pyproject.toml"),
            "[project]\nrequires-python = \">=3.12\"\n",
        )
        .unwrap();
        let mut workspace = Filesystem.load(std::slice::from_ref(&root_path), true);
        workspace.projects[0].python_version = match case["python"].as_str().unwrap() {
            "3.12" => PythonVersion::Py312,
            "3.13" => PythonVersion::Py313,
            "3.14" => PythonVersion::Py314,
            value => panic!("Unexpected target {value}"),
        };
        let report = super::super::check(&workspace);
        let fatal = case["fatal"].as_bool().unwrap();
        assert_eq!(report.checked_files, 1);
        assert_eq!(
            report.completion,
            if fatal {
                CompletionStatus::Error
            } else {
                CompletionStatus::Incomplete
            }
        );
        assert_eq!(report.reference_exit_code.is_some(), fatal);
        let mut actual = json!(report
            .diagnostics
            .iter()
            .map(|value| json!({
                "rule":value.rule,"hint":value.hint,"message":value.message,
                "path":value.path.strip_prefix(&root_path).unwrap(),"severity":value.severity,
                "location":{"line":value.location.line,"column":value.location.column,
                    "end_line":value.location.end_line,"end_column":value.location.end_column},
            }))
            .collect::<Vec<_>>());
        let mut expected = json!(case["original_messages"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| json!({
                "rule":value["messageId"],"hint":null,"message":value["message"],
                "path":value["path"],"severity":value["type"],
                "location":{"line":value["line"],"column":value["column"],
                    "end_line":value["endLine"],"end_column":value["endColumn"]},
            }))
            .collect::<Vec<_>>());
        super::crash_filename::normalize(&mut actual);
        super::crash_filename::normalize(&mut expected);
        let exit = |values: &Value| {
            values.as_array().unwrap().iter().fold(0, |exit, value| {
                exit | match value["severity"].as_str().unwrap() {
                    "fatal" => 1,
                    "error" => 2,
                    "warning" => 4,
                    "refactor" => 8,
                    "convention" => 16,
                    "info" => 0,
                    value => panic!("Unexpected severity {value}"),
                }
            })
        };
        let reference_exit = case["reference_exit"].as_i64().unwrap() as i32;
        assert_eq!(exit(&expected), reference_exit);
        let equal = actual == expected && exit(&actual) == reference_exit;
        if fatal {
            assert_eq!(report.reference_exit_code, Some(exit(&expected)));
        }
        failures += usize::from(!equal);
        results.push(
            json!({"name":case["name"],"filename":name,"python":case["python"],"source_sha256":case["source_sha256"],
            "expected":expected,"actual":actual,"equal":equal,"fatal":fatal,
            "reference_exit":reference_exit,"diagnostic_exit":exit(&actual)}),
        );
        assert_eq!(fs::read(root_path.join(name)).unwrap(), source.as_bytes());
    }
    if let Some(path) = std::env::var_os("MAMBA_QUALITY_PYLINT_BUILD_EVIDENCE") {
        fs::write(
            path,
            serde_json::to_vec_pretty(
                &json!({"cases":results,"failed":failures,"whole_suite_accepted":false}),
            )
            .unwrap(),
        )
        .unwrap();
    }
    assert_eq!(failures, 0, "Pinned slot build probes differ");
}
