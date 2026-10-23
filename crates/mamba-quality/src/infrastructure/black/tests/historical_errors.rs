//! Exact Black API errors and CLI exit projections from pinned original runs.
use super::super::*;
use crate::domain::{ports::ProjectStore, source::PythonVersion};
use crate::infrastructure::filesystem::Filesystem;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs;

#[test]
fn pinned_original_historical_error_messages() {
    let corpus = include_str!("historical_errors.json");
    let cases: Vec<Value> = serde_json::from_str(corpus).unwrap();
    let root = tempfile::tempdir().unwrap();
    let mut results = Vec::new();
    let mut failures = 0;
    for (index, case) in cases.into_iter().enumerate() {
        let path = root.path().join(format!("case_{index}.py"));
        let source = case["source"].as_str().unwrap();
        fs::write(&path, source).unwrap();
        assert_eq!(
            format!("{:x}", Sha256::digest(source.as_bytes())),
            case["source_sha256"].as_str().unwrap()
        );
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
            let expected = &case["expected"][target];
            let actual = match format(&workspace.sources[0], &workspace) {
                Ok(output) => json!({
                    "kind":if output == source { "unchanged" } else { "formatted" },
                    "reference_exit":i32::from(output != source),
                    "exception_type":if output == source { Some("black.report.NothingChanged") } else { None },
                    "message":if output == source { Some("") } else { None },
                    "output":output,
                }),
                Err(error) => json!({
                    "kind":"error","reference_exit":123,
                    "exception_type":error.code,"message":error.message,"output":null,
                }),
            };
            let equal = [
                "kind",
                "reference_exit",
                "exception_type",
                "message",
                "output",
            ]
            .iter()
            .all(|key| actual[*key] == expected[*key]);
            let report = check(&workspace);
            let syntax = quality_syntax::parse(
                source,
                super::super::super::native::target(&workspace, &workspace.sources[0]),
                false,
            )
            .unwrap();
            let report_error_message = expected["kind"] != "error"
                || report.diagnostics.iter().any(|diagnostic| {
                    diagnostic.message == expected["message"].as_str().unwrap()
                        && diagnostic.rule == expected["exception_type"].as_str().unwrap()
                });
            let actual_location = report.diagnostics.first().filter(|diagnostic| {
                diagnostic.rule == "black.parsing.InvalidInput"
            }).map(|diagnostic| json!({"line":diagnostic.location.line,"column":diagnostic.location.column}));
            let location_equal = actual_location.unwrap_or(Value::Null) == expected["location"];
            let equal = equal && report_error_message && location_equal;
            failures += usize::from(!equal);
            results.push(json!({"name":case["name"],"python":version.name(),
                "expected":expected,"actual":actual,"equal":equal,
                "report_error_message":report_error_message,"location_equal":location_equal,
                "syntax_issues":syntax.issues.iter().map(|issue| json!({
                    "message":issue.message,"start":issue.start,"end":issue.end,
                    "version_error":issue.version_error
                })).collect::<Vec<_>>() }));
            assert_eq!(fs::read(&path).unwrap(), source.as_bytes());
        }
    }
    if let Some(destination) = std::env::var_os("MAMBA_QUALITY_BLACK_HISTORICAL_ERRORS_EVIDENCE") {
        fs::write(
            destination,
            serde_json::to_vec_pretty(&json!({
                "schema":"mamba.quality.black-historical-errors-comparison.v1",
                "corpus_sha256":format!("{:x}",Sha256::digest(corpus.as_bytes())),
                "cases":results,"failed":failures,"whole_suite_accepted":false,
            }))
            .unwrap(),
        )
        .unwrap();
    }
    assert_eq!(failures, 0, "Pinned Black error comparisons failed");
}
