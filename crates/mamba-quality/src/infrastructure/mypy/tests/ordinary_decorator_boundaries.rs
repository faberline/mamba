//! Fixed original strict diagnostics for ordinary untyped-decorator boundaries.
use crate::domain::{model::CompletionStatus, ports::ProjectStore, source::PythonVersion};
use crate::infrastructure::filesystem::Filesystem;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs;

#[test]
fn pinned_original_ordinary_decorator_boundaries() {
    let corpus = include_str!("ordinary_decorator_boundaries.json");
    assert_eq!(
        format!("{:x}", Sha256::digest(corpus.as_bytes())),
        "3b937b00c77d5a07802d73d7ca82657afd97cfd29fd72327fa2036716309ceda"
    );
    let cases: Vec<Value> = serde_json::from_str(corpus).unwrap();
    assert_eq!(cases.len(), 7);
    let mut rows = Vec::new();
    let mut failures = 0usize;
    for case in cases {
        let source = case["source"].as_str().unwrap();
        assert_eq!(
            format!("{:x}", Sha256::digest(source.as_bytes())),
            case["source_sha256"].as_str().unwrap()
        );
        let root = tempfile::tempdir().unwrap();
        let root_path = fs::canonicalize(root.path()).unwrap();
        let path = root_path.join(case["filename"].as_str().unwrap());
        fs::write(&path, source).unwrap();
        let mut workspace = Filesystem.load(std::slice::from_ref(&path), true);
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
                        "path":d.path.strip_prefix(&root_path).unwrap(), "rule":d.rule,
                        "message":d.message, "hint":d.hint, "severity":d.severity,
                        "location":{"line":d.location.line,"column":d.location.column,
                            "end_line":d.location.end_line,"end_column":d.location.end_column}
                    })
                })
                .collect();
            let actual = Value::Array(actual);
            let expected = &case["expected"][target];
            let diagnostic_exit =
                i64::from(report.diagnostics.iter().any(|d| d.severity == "error"));
            let reference_exit = case["reference_exits"][target].as_i64().unwrap();
            let source_preserved = fs::read(&path).unwrap() == source.as_bytes();
            let equal = actual == *expected
                && diagnostic_exit == reference_exit
                && source_preserved
                && report.completion == CompletionStatus::Incomplete
                && report.reference_exit_code.is_none()
                && report.checked_files == 1;
            failures += usize::from(!equal);
            rows.push(json!({"case":case["name"],"python":version.name(),"expected":expected,
                "actual":actual,"reference_exit":reference_exit,"diagnostic_exit":diagnostic_exit,
                "completion":report.completion,"native_reference_exit":report.reference_exit_code,
                "checked_files":report.checked_files,"source_preserved":source_preserved,"equal":equal}));
        }
    }
    assert_eq!(rows.len(), 21);
    if let Some(destination) =
        std::env::var_os("MAMBA_QUALITY_MYPY_ORDINARY_DECORATOR_BOUNDARY_EVIDENCE")
    {
        let file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(destination)
            .unwrap();
        serde_json::to_writer_pretty(file, &json!({"schema":"mamba.quality.mypy-ordinary-decorator-boundary-comparison.v1",
            "fixture_sha256":"3b937b00c77d5a07802d73d7ca82657afd97cfd29fd72327fa2036716309ceda","cases":rows,"failed":failures,"whole_plan_accepted":false})).unwrap();
    }
    assert_eq!(
        failures, 0,
        "Frozen ordinary-decorator original comparisons failed"
    );
}
