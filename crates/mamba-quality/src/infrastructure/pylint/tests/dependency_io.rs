//! Dependency I/O failures retain originals and restore private permissions.
#![cfg(unix)]
use crate::domain::{model::CompletionStatus, ports::ProjectStore, source::PythonVersion};
use crate::infrastructure::filesystem::Filesystem;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{fs, os::unix::fs::PermissionsExt, path::PathBuf};

struct Restricted(PathBuf, fs::Permissions);
impl Drop for Restricted {
    fn drop(&mut self) {
        fs::set_permissions(&self.0, self.1.clone()).unwrap();
    }
}

#[test]
fn pinned_original_dependency_io() {
    let cases: Vec<Value> = serde_json::from_str(include_str!("dependency_io.json")).unwrap();
    let mut results = Vec::new();
    for case in cases {
        let directory = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(directory.path()).unwrap();
        fs::write(
            root.join("pyproject.toml"),
            "[project]\nrequires-python = '>=3.12'\n",
        )
        .unwrap();
        for (name, text) in case["files"].as_object().unwrap() {
            assert_eq!(
                format!("{:x}", Sha256::digest(text.as_str().unwrap())),
                case["source_sha256"][name]
            );
            let path = root.join(name);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, text.as_str().unwrap()).unwrap();
        }
        let path = root.join(case["restricted"].as_str().unwrap());
        let restricted = Restricted(path.clone(), fs::metadata(&path).unwrap().permissions());
        fs::set_permissions(&path, fs::Permissions::from_mode(0o0)).unwrap();
        let paths: Vec<_> = case["inputs"]
            .as_array()
            .unwrap()
            .iter()
            .map(|name| root.join(name.as_str().unwrap()))
            .collect();
        let mut workspace = Filesystem.load(&paths, true);
        for (target, version) in [
            PythonVersion::Py312,
            PythonVersion::Py313,
            PythonVersion::Py314,
        ]
        .into_iter()
        .enumerate()
        {
            workspace.projects[0].python_version = version;
            let report = super::super::check(&workspace);
            let actual: Vec<_> = report
                .diagnostics
                .iter()
                .map(|d| {
                    json!({
                        "rule":d.rule,"message":d.message,"hint":d.hint,
                        "path":d.path.strip_prefix(&root).unwrap(),"severity":d.severity,
                        "location":{"line":d.location.line,"column":d.location.column,
                            "end_line":d.location.end_line,"end_column":d.location.end_column},
                    })
                })
                .collect();
            let mut actual = Value::Array(actual);
            let mut expected = case["expected"][target].clone();
            super::crash_filename::normalize(&mut actual);
            super::crash_filename::normalize(&mut expected);
            assert_eq!(actual, expected);
            let exit = report.diagnostics.iter().fold(0, |exit, diagnostic| {
                exit | match diagnostic.severity.as_str() {
                    "error" => 2,
                    "fatal" => 1,
                    "warning" => 4,
                    "convention" => 16,
                    "info" => 0,
                    other => panic!("Unexpected I/O severity: {other}"),
                }
            });
            assert_eq!(case["reference_exits"][target], exit);
            assert_eq!(report.checked_files, 1);
            let fatal = report.diagnostics.iter().any(|value| value.rule == "F0002");
            assert_eq!(report.reference_exit_code, fatal.then_some(exit));
            let crash_reports: Vec<_> = report
                .diagnostics
                .iter()
                .filter(|diagnostic| diagnostic.rule == "F0002")
                .map(|diagnostic| {
                    let path = diagnostic.message.rsplit_once("in '").unwrap().1.strip_suffix("'.").unwrap();
                    let bytes = fs::read(path).unwrap();
                    let contents = std::str::from_utf8(&bytes).unwrap();
                    assert!(contents.contains("PermissionError"));
                    assert!(contents.contains("isort source classification"));
                    assert!(contents.contains(case["files"]["main.py"].as_str().unwrap()));
                    json!({"path":path,"sha256":format!("{:x}", Sha256::digest(&bytes)),"bytes":bytes.len()})
                })
                .collect();
            let error = case["expected_completion"]
                .as_array()
                .is_none_or(|values| values[target] == "error");
            assert_eq!(
                report.completion,
                if error {
                    CompletionStatus::Error
                } else {
                    CompletionStatus::Incomplete
                },
                "Completion differs for {} on {}: {:?}",
                case["name"],
                version.name(),
                report.remaining
            );
            if error && !fatal {
                assert!(report.remaining.iter().any(|message| message
                    .contains(case["restricted"].as_str().unwrap())
                    && message.contains("Permission denied")));
            }
            results.push(json!({"name":case["name"],"python":version.name(),
                "expected":case["expected"][target],"actual":actual,"diagnostic_exit":exit,
                "reference_exit":case["reference_exits"][target],"completion":report.completion,
                "remaining":report.remaining,"crash_reports":crash_reports,
                "checked_files":report.checked_files,"equal":true}));
        }
        drop(restricted);
        for (name, text) in case["files"].as_object().unwrap() {
            assert_eq!(
                fs::read_to_string(root.join(name)).unwrap(),
                text.as_str().unwrap()
            );
        }
    }
    if let Some(path) = std::env::var_os("MAMBA_QUALITY_PYLINT_DEPENDENCY_IO_EVIDENCE") {
        fs::write(
            path,
            serde_json::to_vec_pretty(
                &json!({"cases":results,"failed":0,"whole_suite_accepted":false}),
            )
            .unwrap(),
        )
        .unwrap();
    }
}
