//! Predicate branch diagnostics retain all original fields for three Python targets.
use crate::domain::{model::CompletionStatus, ports::ProjectStore, source::PythonVersion};
use crate::infrastructure::filesystem::Filesystem;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs;

#[test]
fn pinned_original_predicate_bindings() {
    let corpus = include_str!("predicate_bindings.json");
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
        let mut workspace = Filesystem.load(std::slice::from_ref(&root_path), true);
        assert_eq!(workspace.sources.len(), files.len());
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
            assert_eq!(report.completion, CompletionStatus::Incomplete);
            assert_eq!(report.reference_exit_code, None);
            let checked_files_match = report.checked_files == files.len();
            let actual: Vec<_> = report.diagnostics.iter().map(|diagnostic| json!({
                "rule": serde_json::to_value(diagnostic).unwrap()["rule"], "message": diagnostic.message, "hint": diagnostic.hint,
                "path": diagnostic.path.strip_prefix(&root_path).unwrap(), "severity": diagnostic.severity,
                "location": {"line": diagnostic.location.line, "column": diagnostic.location.column,
                    "end_line": diagnostic.location.end_line, "end_column": diagnostic.location.end_column},
            })).collect();
            // Pinned mypy JSON output returns one for any emitted message, including notes.
            let diagnostic_exit = i32::from(!actual.is_empty());
            let actual = Value::Array(actual);
            let expected = &case["expected"][target];
            let equal = checked_files_match
                && actual == *expected
                && case["reference_exits"][target].as_i64() == Some(i64::from(diagnostic_exit));
            failures += usize::from(!equal);
            results.push(json!({
                "name":case["name"],"files":files,"python":version.name(),
                "expected":expected,"actual":actual,"equal":equal,
                "reference_exit":case["reference_exits"][target],"diagnostic_exit":diagnostic_exit,
                "checked_files":report.checked_files,"checked_files_match":checked_files_match,
            }));
            for (filename, contents) in files {
                assert_eq!(
                    fs::read(root_path.join(filename)).unwrap(),
                    contents.as_str().unwrap().as_bytes()
                );
            }
        }
    }
    if let Some(destination) = std::env::var_os("MAMBA_QUALITY_MYPY_PREDICATE_BINDING_EVIDENCE") {
        fs::write(
            destination,
            serde_json::to_vec_pretty(&json!({
                "schema":"mamba.quality.mypy-predicate-binding-comparison.v1",
                "corpus_sha256":format!("{:x}",Sha256::digest(corpus.as_bytes())),
                "cases":results,"failed":failures,"whole_suite_accepted":false,
            }))
            .unwrap(),
        )
        .unwrap();
    }
    assert_eq!(failures, 0, "Pinned predicate comparisons failed");
}
