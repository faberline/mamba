//! Frozen strict mypy controls cover typed star arguments and contextual overloads.

use crate::domain::{model::CompletionStatus, ports::ProjectStore, source::PythonVersion};
use crate::infrastructure::filesystem::Filesystem;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs;

#[test]
fn pinned_math_binding_control_comparisons() {
    let corpus = include_str!("math_binding_controls.json");
    let cases: Vec<Value> = serde_json::from_str(corpus).unwrap();
    assert_eq!(cases.len(), 8);
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
        assert_eq!(files.len(), 1);
        assert!(files.contains_key("main.py"));
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
        for (filename, contents) in case["local"].as_object().unwrap() {
            assert_eq!(
                format!(
                    "{:x}",
                    Sha256::digest(contents.as_str().unwrap().as_bytes())
                ),
                case["local_sha256"][filename].as_str().unwrap(),
            );
            let path = root_path.join(filename);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, contents.as_str().unwrap()).unwrap();
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
            let selected: Vec<_> = files.keys().map(|name| root_path.join(name)).collect();
            let workspace = Filesystem.load(&selected, true);
            assert_eq!(workspace.sources.len(), files.len());
            let report = super::super::check(&workspace);
            assert_eq!(report.completion, CompletionStatus::Incomplete);
            assert_eq!(report.reference_exit_code, None);
            assert_eq!(report.checked_files, 1);
            for (filename, contents) in files {
                assert_eq!(
                    fs::read(root_path.join(filename)).unwrap(),
                    contents.as_str().unwrap().as_bytes(),
                );
            }
            for (filename, contents) in case["local"].as_object().unwrap() {
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
            let actual: Vec<_> = report.diagnostics.iter().map(|diagnostic| json!({
                "rule": serde_json::to_value(diagnostic).unwrap()["rule"], "message": diagnostic.message, "hint": diagnostic.hint,
                "path": diagnostic.path.strip_prefix(&root_path).unwrap(), "severity": diagnostic.severity,
                "location": {"line": serde_json::to_value(&diagnostic.location).unwrap()["line"], "column": diagnostic.location.column,
                    "end_line": serde_json::to_value(&diagnostic.location).unwrap()["end_line"], "end_column": diagnostic.location.end_column},
            })).collect();
            let diagnostic_exit = i32::from(!actual.is_empty());
            let actual = Value::Array(actual);
            let expected = &case["expected"][target];
            let equal = actual == *expected
                && case["reference_exits"][target].as_i64() == Some(i64::from(diagnostic_exit));
            failures += usize::from(!equal);
            results.push(
                json!({"name": case["name"], "files": files, "python": version.name(),
                "expected": expected, "actual": actual, "equal": equal,
                "reference_exit": case["reference_exits"][target], "actual_completion": report.completion,
                "actual_reference_exit": report.reference_exit_code, "checked_files": report.checked_files,
                "diagnostic_exit": diagnostic_exit}),
            );
        }
    }
    if let Some(destination) = std::env::var_os("MAMBA_QUALITY_MYPY_MATH_BINDING_CONTROL_EVIDENCE")
    {
        fs::write(destination, serde_json::to_vec_pretty(&json!({
            "schema": "mamba.quality.mypy-math-binding-controls-comparison.v1", "corpus_sha256": format!("{:x}",Sha256::digest(corpus.as_bytes())),
            "cases": results, "failed": failures, "whole_suite_accepted": false,
        })).unwrap()).unwrap();
    }
    assert_eq!(results.len(), 24);
    assert_eq!(failures, 0, "Pinned math binding comparisons failed");
}
