//! Cross-file writes retain the original full diagnostic lists and tool exits.
use crate::domain::{ports::ProjectStore, source::PythonVersion};
use crate::infrastructure::filesystem::Filesystem;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs;

#[test]
fn pinned_original_cross_file_stores() {
    compare(include_str!("cross_file_stores.json"), "stores");
}

#[test]
fn pinned_original_cross_file_interactions() {
    compare(include_str!("cross_file_interactions.json"), "interactions");
}

fn compare(contents: &str, scope: &str) {
    let cases: Vec<Value> = serde_json::from_str(contents).unwrap();
    let mut results = Vec::new();
    let mut failures = 0;
    for case in cases {
        let directory = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(directory.path()).unwrap();
        fs::write(
            root.join("pyproject.toml"),
            "[project]\nrequires-python = '>=3.12'\n",
        )
        .unwrap();
        let files = case["files"].as_object().unwrap();
        for (name, contents) in files {
            let text = contents.as_str().unwrap();
            assert_eq!(
                format!("{:x}", Sha256::digest(text)),
                case["source_sha256"][name]
            );
            let path = root.join(name);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, text).unwrap();
        }
        let paths: Vec<_> = case["inputs"].as_array().map_or_else(
            || vec![root.clone()],
            |inputs| {
                inputs
                    .iter()
                    .map(|name| root.join(name.as_str().unwrap()))
                    .collect()
            },
        );
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
            assert_eq!(
                report.checked_files,
                workspace
                    .sources
                    .iter()
                    .filter(|source| !source.is_stub)
                    .count()
            );
            assert_eq!(
                report.completion,
                crate::domain::model::CompletionStatus::Incomplete
            );
            assert_eq!(report.reference_exit_code, None);
            let actual: Vec<_> = report
                .diagnostics
                .iter()
                .map(|d| {
                    json!({
                        "rule":d.rule,"message":d.message.replace(root.to_str().unwrap(), "<project>"),"hint":d.hint,
                        "path":d.path.strip_prefix(&root).unwrap(),"severity":d.severity,
                        "location":{"line":d.location.line,"column":d.location.column,
                            "end_line":d.location.end_line,"end_column":d.location.end_column},
                    })
                })
                .collect();
            let exit = report.diagnostics.iter().fold(0, |code, d| {
                code | match d.severity.as_str() {
                    "fatal" => 1,
                    "error" => 2,
                    "warning" => 4,
                    "refactor" => 8,
                    "convention" => 16,
                    "info" => 0,
                    other => panic!("Unexpected severity: {other}"),
                }
            });
            let equal = actual == case["expected"][target].as_array().unwrap().as_slice()
                && exit == case["reference_exits"][target].as_i64().unwrap();
            failures += usize::from(!equal);
            results.push(json!({"name":case["name"],"python":version.name(),
                "expected":case["expected"][target],"actual":actual,
                "reference_exit":case["reference_exits"][target],"diagnostic_exit":exit,
                "equal":equal}));
            for (name, contents) in files {
                assert_eq!(
                    fs::read(root.join(name)).unwrap(),
                    contents.as_str().unwrap().as_bytes()
                );
            }
        }
    }
    if let Some(path) = std::env::var_os("MAMBA_QUALITY_PYLINT_CROSS_FILE_EVIDENCE") {
        let path = std::path::PathBuf::from(path).with_extension(format!("{scope}.json"));
        fs::write(
            path,
            serde_json::to_vec_pretty(&json!({"cases":results,"failed":failures,
            "whole_suite_accepted":false}))
            .unwrap(),
        )
        .unwrap();
    }
    assert_eq!(failures, 0, "Cross-file original diagnostic lists differ");
}

#[test]
fn pinned_original_cross_file_slot_and_instance_interactions() {
    compare(include_str!("cross_file_mutations.json"), "mutations");
}

#[test]
fn pinned_original_cross_file_attribute_names() {
    compare(include_str!("cross_file_names.json"), "names");
}

#[test]
fn pinned_original_cross_file_import_loading() {
    compare(include_str!("cross_file_imports.json"), "imports");
}

#[test]
fn pinned_original_selected_import_sources() {
    compare(
        include_str!("cross_file_selected_imports.json"),
        "selected_imports",
    );
}

#[test]
fn pinned_original_cross_file_dependency_data() {
    compare(include_str!("cross_file_dependencies.json"), "dependencies");
}

#[test]
fn pinned_original_cross_file_dependency_regressions() {
    compare(
        include_str!("cross_file_dependency_regressions.json"),
        "dependency_regressions",
    );
}

#[test]
fn pinned_original_import_precedence() {
    compare(
        include_str!("cross_file_import_precedence.json"),
        "import_precedence",
    );
}

#[test]
fn pinned_original_relative_import_boundaries() {
    compare(
        include_str!("cross_file_relative_boundaries.json"),
        "relative_boundaries",
    );
}

#[test]
fn pinned_original_stub_dependencies() {
    compare(include_str!("cross_file_stubs.json"), "stubs");
}
