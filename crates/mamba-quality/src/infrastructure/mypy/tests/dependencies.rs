//! Multi-file originals retain every diagnostic and the original file order.

use crate::domain::{model::CompletionStatus, ports::ProjectStore, source::PythonVersion};
use crate::infrastructure::filesystem::Filesystem;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs;

#[test]
fn pinned_original_dependencies() {
    let corpus = include_str!("dependencies.json");
    let cases: Vec<Value> = serde_json::from_str(corpus).unwrap();
    let mut results = Vec::new();
    let mut failures = 0;
    for case in cases {
        let root = tempfile::tempdir().unwrap();
        let root_path = fs::canonicalize(root.path()).unwrap();
        fs::write(
            root_path.join("pyproject.toml"),
            "[project]\nname = \"fixture\"\nversion = \"0.0.0\"\nrequires-python = \">=3.12\"\n",
        )
        .unwrap();
        let files = case["files"].as_object().unwrap();
        for (filename, contents) in files {
            let source = contents.as_str().unwrap();
            assert_eq!(
                format!("{:x}", Sha256::digest(source.as_bytes())),
                case["source_sha256"][filename].as_str().unwrap()
            );
            let path = root_path.join(filename);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, source).unwrap();
        }
        for (target, version) in [
            PythonVersion::Py312,
            PythonVersion::Py313,
            PythonVersion::Py314,
        ]
        .into_iter()
        .enumerate()
        {
            let package_root =
                root_path.join(format!(".venv/lib/python{}/site-packages", version.name()));
            fs::create_dir_all(&package_root).unwrap();
            fs::write(
                root_path.join(".venv/pyvenv.cfg"),
                format!("version = {}.0\n", version.name()),
            )
            .unwrap();
            for (name, contents) in case["packages"].as_object().unwrap() {
                let source = contents.as_str().unwrap();
                assert_eq!(
                    format!("{:x}", Sha256::digest(source.as_bytes())),
                    case["package_sha256"][name].as_str().unwrap()
                );
                let path = package_root.join(name);
                fs::create_dir_all(path.parent().unwrap()).unwrap();
                fs::write(path, source).unwrap();
            }
            fs::write(
                root_path.join("pyproject.toml"),
                format!("[project]\nrequires-python = \">={}\"\n", version.name()),
            )
            .unwrap();
            let workspace = Filesystem.load(std::slice::from_ref(&root_path), true);
            assert_eq!(workspace.sources.len(), files.len());
            let report = super::super::check(&workspace);
            for (filename, contents) in files {
                assert_eq!(
                    fs::read(root_path.join(filename)).unwrap(),
                    contents.as_str().unwrap().as_bytes(),
                );
            }
            for (filename, contents) in case["packages"].as_object().unwrap() {
                assert_eq!(
                    fs::read(package_root.join(filename)).unwrap(),
                    contents.as_str().unwrap().as_bytes(),
                );
            }
            assert_eq!(report.completion, CompletionStatus::Incomplete);
            assert_eq!(report.reference_exit_code, None);
            assert_eq!(report.checked_files, files.len());
            let actual: Vec<_> = report.diagnostics.iter().map(|diagnostic| json!({
                "rule": serde_json::to_value(diagnostic).unwrap()["rule"], "message": diagnostic.message, "hint": diagnostic.hint,
                "path": diagnostic.path.strip_prefix(&root_path).unwrap(), "severity": diagnostic.severity,
                "location": {"line": diagnostic.location.line, "column": diagnostic.location.column,
                    "end_line": diagnostic.location.end_line, "end_column": diagnostic.location.end_column},
            })).collect();
            let actual = Value::Array(actual);
            let expected = &case["expected"][target];
            let equal = actual == *expected;
            failures += usize::from(!equal);
            results.push(
                json!({"name": case["name"], "files": files, "python": version.name(),
                "expected": expected, "actual": actual, "equal": equal,
                "reference_exit": case["reference_exits"][target]}),
            );
        }
    }
    if let Some(destination) = std::env::var_os("MAMBA_QUALITY_MYPY_DEPENDENCY_EVIDENCE") {
        fs::write(destination, serde_json::to_vec_pretty(&json!({
            "schema": "mamba.quality.mypy-dependency-comparison.v1", "corpus_sha256": format!("{:x}",Sha256::digest(corpus.as_bytes())),
            "cases": results, "failed": failures, "whole_suite_accepted": false,
        })).unwrap()).unwrap();
    }
    assert_eq!(
        failures, 0,
        "Pinned multi-file mypy diagnostic comparisons failed"
    );
}

#[test]
fn exported_uppercase_class_does_not_alias_a_lowercase_module_on_the_host_filesystem() {
    let root = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(root.path()).unwrap();
    let main = root.join("main.py");
    fs::write(
        &main,
        "from qcase import Session\n\nvalue: Session = Session()\n",
    )
    .unwrap();
    let mut reports = Vec::new();

    for (version, minor) in [
        (PythonVersion::Py312, "3.12"),
        (PythonVersion::Py313, "3.13"),
        (PythonVersion::Py314, "3.14"),
    ] {
        let site = root.join(format!(".venv/lib/python{minor}/site-packages"));
        let package = site.join("qcase");
        fs::create_dir_all(&package).unwrap();
        fs::write(
            root.join(".venv/pyvenv.cfg"),
            format!("version = {minor}.0\n"),
        )
        .unwrap();
        fs::write(
            root.join("pyproject.toml"),
            format!("[project]\nrequires-python = '>={minor}'\n"),
        )
        .unwrap();
        fs::write(
            package.join("__init__.py"),
            "from .session import Session as Session\n",
        )
        .unwrap();
        fs::write(package.join("session.py"), "class Session:\n    pass\n").unwrap();
        fs::write(package.join("py.typed"), b"").unwrap();

        let workspace = Filesystem.load(std::slice::from_ref(&main), true);
        assert_eq!(workspace.sources.len(), 1);
        assert_eq!(workspace.projects[0].python_version, version);
        let report = super::super::check(&workspace);
        assert_eq!(report.completion, CompletionStatus::Incomplete);
        assert_eq!(report.reference_exit_code, None);
        assert_eq!(report.checked_files, 1);
        assert!(!report.diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("Source file found twice under different module names")));
        reports.push(report);
    }

    assert!(reports.iter().all(|report| report.diagnostics.is_empty()));
}
