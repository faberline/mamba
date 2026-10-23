//! Entry comparisons preserve missing inputs independently of source fixtures.
use crate::domain::{model::*, ports::ProjectStore};
use crate::infrastructure::filesystem::Filesystem;
use serde_json::{json, Value};
use std::{fs, path::Path};

#[test]
fn pinned_missing_input_lists() {
    let records: Vec<Value> = serde_json::from_str(include_str!("entry_inputs_data.json")).unwrap();
    assert_eq!(records.len(), 21);
    for record in records {
        let root = tempfile::tempdir().unwrap();
        fs::write(
            root.path().join("pyproject.toml"),
            format!(
                "[project]\nrequires-python = '>= {}'\n",
                record["python"].as_str().unwrap()
            ),
        )
        .unwrap();
        for (name, source) in record["files"].as_object().unwrap() {
            fs::write(root.path().join(name), source.as_str().unwrap()).unwrap();
        }
        let mut workspace = Filesystem.load(&[root.path().into()], true);
        for input in record["inputs"].as_array().unwrap() {
            let name = input.as_str().unwrap();
            if record["files"].get(name).is_some() {
                continue;
            }
            let path = if record["absolute_inputs"].as_bool().unwrap() {
                root.path().join(name)
            } else {
                name.into()
            };
            assert!(fs::metadata(&path)
                .is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound));
            workspace
                .issues
                .push(QualityIssue::new("path-read", "missing input", Some(path)));
        }
        let report = super::super::check(&workspace);
        assert_eq!(report.completion, CompletionStatus::Error);
        assert_eq!(report.reference_exit_code, None);
        assert_eq!(
            report.checked_files,
            record["files"].as_object().unwrap().len()
        );
        let actual: Vec<_> = report
            .diagnostics
            .iter()
            .map(|diagnostic| {
                let path = diagnostic
                    .path
                    .strip_prefix(root.path())
                    .unwrap_or(Path::new(&diagnostic.path));
                assert!(diagnostic.fix.is_none() && diagnostic.hint.is_none());
                json!({"rule": diagnostic.rule, "message": diagnostic.message, "path": path,
                   "severity": diagnostic.severity, "line": diagnostic.location.line,
                   "column": diagnostic.location.column, "end_line": diagnostic.location.end_line,
                   "end_column": diagnostic.location.end_column})
            })
            .collect();
        assert_eq!(
            Value::Array(actual),
            record["expected"],
            "{} {}",
            record["python"],
            record["name"]
        );
        assert_eq!(record["reference_exit"], 1);
        assert!(report
            .diagnostics
            .iter()
            .all(|diagnostic| diagnostic.severity == "fatal"));
    }
}
