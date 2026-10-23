//! Selected path errors retain the caller input and original fatal diagnostic.
#![cfg(unix)]
use crate::domain::{model::*, ports::ProjectStore, profile::FixedProfile};
use crate::infrastructure::filesystem::Filesystem;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{fs, path::PathBuf, process::Command};

const CHILD_TEST: &str = "infrastructure::pylint::tests::entry_path_errno::pinned_case_child";
const CASE_ENV: &str = "MAMBA_QUALITY_ENTRY_ERRNO_CASE_INDEX";

fn cases() -> Vec<Value> {
    serde_json::from_str(include_str!("entry_path_errno_data.json")).unwrap()
}

#[test]
fn pinned_selected_path_errno() {
    let records = cases();
    assert_eq!(records.len(), 18);
    for (index, record) in records.iter().enumerate() {
        let root = tempfile::tempdir().unwrap();
        let root_path = root.path().canonicalize().unwrap();
        for (name, source) in record["files"].as_object().unwrap() {
            let path = root_path.join(name);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            let bytes = source["text"].as_str().unwrap().as_bytes();
            assert_eq!(format!("{:x}", Sha256::digest(bytes)), source["sha256"]);
            fs::write(path, bytes).unwrap();
        }
        for (name, target) in record["symlinks"].as_object().unwrap() {
            std::os::unix::fs::symlink(target.as_str().unwrap(), root_path.join(name)).unwrap();
        }
        let output = Command::new(std::env::current_exe().unwrap())
            .arg("--exact")
            .arg(CHILD_TEST)
            .env(CASE_ENV, index.to_string())
            .current_dir(&root_path)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{} {}: status={} stdout={} stderr={}",
            record["python"],
            record["name"],
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        for (name, source) in record["files"].as_object().unwrap() {
            assert_eq!(
                fs::read(root_path.join(name)).unwrap(),
                source["text"].as_str().unwrap().as_bytes()
            );
        }
        for (name, target) in record["symlinks"].as_object().unwrap() {
            assert_eq!(
                fs::read_link(root_path.join(name)).unwrap(),
                PathBuf::from(target.as_str().unwrap())
            );
        }
    }
}

#[test]
fn pinned_case_child() {
    let Ok(index) = std::env::var(CASE_ENV) else {
        return;
    };
    let index: usize = index.parse().unwrap();
    let record = &cases()[index];
    let root = std::env::current_dir().unwrap();
    let absolute_inputs = record["absolute_inputs"].as_bool().unwrap_or(false);
    let inputs: Vec<PathBuf> = record["inputs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|input| {
            let path = PathBuf::from(input.as_str().unwrap());
            if absolute_inputs {
                root.join(path)
            } else {
                path
            }
        })
        .collect();
    assert_eq!(inputs.len(), 1);
    let portable_input = PathBuf::from(record["inputs"][0].as_str().unwrap());
    assert_eq!(
        inputs[0],
        if absolute_inputs {
            root.join(&portable_input)
        } else {
            portable_input
        }
    );
    let error = fs::metadata(&inputs[0]).unwrap_err();
    match record["expected_errno_name"].as_str().unwrap() {
        "ENOTDIR" => assert_eq!(error.kind(), std::io::ErrorKind::NotADirectory),
        "ENAMETOOLONG" => assert_eq!(error.kind(), std::io::ErrorKind::InvalidFilename),
        "ELOOP" => {
            #[cfg(target_os = "macos")]
            assert_eq!(error.raw_os_error(), Some(62));
            #[cfg(target_os = "linux")]
            assert_eq!(error.raw_os_error(), Some(40));
        }
        name => panic!("Unexpected selected errno: {name}"),
    }
    let workspace = Filesystem.load(&inputs, true);
    assert_eq!(workspace.issues.len(), 1);
    assert_eq!(workspace.issues[0].code, "path-read");
    assert_eq!(workspace.issues[0].path.as_ref(), Some(&inputs[0]));
    assert!(workspace.sources.is_empty());
    let report = super::super::check(&workspace);
    assert_eq!(report.completion, CompletionStatus::Error);
    assert_eq!(report.reference_exit_code, None);
    assert_eq!(report.checked_files, 0);
    let actual: Vec<_> = report
        .diagnostics
        .iter()
        .map(|diagnostic| {
            assert!(diagnostic.hint.is_none() && diagnostic.fix.is_none());
            assert_eq!(diagnostic.path, inputs[0]);
            assert!(
                diagnostic.location.offset.is_none() && diagnostic.location.end_offset.is_none()
            );
            let portable_path = diagnostic
                .path
                .strip_prefix(&root)
                .unwrap_or(&diagnostic.path);
            json!({"rule":diagnostic.rule,"message":diagnostic.message,"path":portable_path,
            "severity":diagnostic.severity,"line":diagnostic.location.line,
            "column":diagnostic.location.column,"end_line":diagnostic.location.end_line,
            "end_column":diagnostic.location.end_column})
        })
        .collect();
    assert_eq!(Value::Array(actual), record["expected"]);
    let diagnostic_exit = report.diagnostics.iter().fold(0, |exit, diagnostic| {
        exit | match diagnostic.severity.as_str() {
            "fatal" => 1,
            severity => panic!("Unexpected severity: {severity}"),
        }
    });
    assert_eq!(diagnostic_exit, record["reference_exit"].as_i64().unwrap());
    let mut overall = QualityReport {
        schema: REPORT_SCHEMA.into(),
        operation: QualityOperation::Check,
        profile: FixedProfile::locked(),
        projects: workspace.projects.clone(),
        tools: vec![report],
        issues: workspace.issues.clone(),
        changes: Vec::new(),
        files: FileCounts::default(),
        analysis_completed: false,
        passed: false,
        exit_code: 2,
    };
    overall.finish();
    assert_eq!(overall.exit_code, 2);
}
