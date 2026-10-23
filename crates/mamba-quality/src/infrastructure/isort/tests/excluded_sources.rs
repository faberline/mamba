//! Recovered original inputs retain exact API output and independent checker exits.
use super::super::*;
use crate::domain::{ports::ProjectStore, source::PythonVersion};
use crate::infrastructure::filesystem::Filesystem;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs;

#[test]
fn pinned_original_excluded_sources() {
    let corpus = include_str!("excluded_sources.json");
    let cases: Vec<Value> = serde_json::from_str(corpus).unwrap();
    let root = tempfile::tempdir().unwrap();
    let mut results = Vec::new();
    let mut failures = 0;
    for (index, case) in cases.into_iter().enumerate() {
        let path = root.path().join(format!("case_{index}.py"));
        let source = case["source"].as_str().unwrap();
        fs::write(&path, source).unwrap();
        let source_bytes_before = fs::read(&path).unwrap();
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
            let actual = match format(&workspace.sources[0], &workspace, &FixedProfile::locked()) {
                Ok((output, remaining)) => json!({
                    "output":output,"remaining":remaining,
                    "error_class":null,"message":null,
                    "reference_exit":i32::from(model::reference_changed(
                        &workspace.sources[0], &workspace, &FixedProfile::locked(), &output)),
                }),
                Err(error) => json!({
                    "output":null,"remaining":[],"error_class":error.code,
                    "message":error.message,
                    "reference_exit":if error.code == "ValueError" { 0 } else { 1 },
                }),
            };
            let api_equal = ["output", "error_class", "message"]
                .iter()
                .all(|key| actual[*key] == expected[*key]);
            let transformed = transform(&workspace, &FixedProfile::locked());
            let issue_equal = expected.get("error_class").is_none()
                || transformed.issues.iter().any(|issue| {
                    issue.code == expected["error_class"].as_str().unwrap()
                        && issue.message == expected["message"].as_str().unwrap()
                });
            let cli_equal = actual["reference_exit"] == expected["reference_exit"];
            let checked = check(&workspace, &FixedProfile::locked());
            let check_equal = if expected.get("error_class").is_none() {
                checked.remaining.len() == 1
                    && i32::from(!checked.diagnostics.is_empty())
                        == expected["reference_exit"].as_i64().unwrap() as i32
            } else {
                checked
                    .remaining
                    .iter()
                    .any(|message| Some(message.as_str()) == expected["message"].as_str())
            };
            let equal = api_equal && issue_equal && cli_equal && check_equal;
            failures += usize::from(!equal);
            let normalized = source.to_owned();
            let syntax =
                quality_syntax::parse(&normalized, version_to_target(version), false).unwrap();
            let source_bytes_after = fs::read(&path).unwrap();
            let source_sha256 = format!("{:x}", Sha256::digest(&source_bytes_before));
            let transform_output = transformed
                .candidates
                .iter()
                .map(|(source_index, output)| {
                    json!({
                        "source_index": source_index,
                        "output": output,
                        "output_bytes_hex": bytes_hex(output.as_bytes()),
                    })
                })
                .collect::<Vec<_>>();
            let transform_record = json!({
                "completion": transformed.completion,
                "output": transform_output,
                "issues": transformed.issues.iter().map(serde_json::to_value).collect::<Result<Vec<_>, _>>().unwrap(),
                "remaining": transformed.remaining,
            });
            results.push(json!({"name":case["name"],"python":version.name(),
                "expected":expected,"actual":actual.clone(),"equal":equal,"issue_equal":issue_equal,
                "completion":transformed.completion,"reference_cli_exit":expected["reference_exit"],"native_exit_projection":actual["reference_exit"],"public_cli_exit_exact":actual["reference_exit"] == expected["reference_exit"],
                "native_public_check_equal":check_equal,
                "executed": {
                    "path": path,
                    "source_path_before": path,
                    "source_path_after": path,
                    "python_version": version.name(),
                    "target": version.name(),
                    "source": source,
                    "source_sha256": source_sha256,
                    "source_bytes_hex": bytes_hex(&source_bytes_before),
                    "source_bytes_len": source_bytes_before.len(),
                    "source_snapshot_path": workspace.sources[0].path.display().to_string(),
                    "source_snapshot_sha256": workspace.sources[0].sha256.clone(),
                    "source_snapshot_bytes_hex": bytes_hex(&workspace.sources[0].bytes),
                    "format_output_bytes_hex": actual["output"].as_str().map(|output| bytes_hex(output.as_bytes())),
                    "transform_result": transform_record,
                    "check_tool_report": serde_json::to_value(&checked).unwrap(),
                    "disk_source_after_bytes_hex": bytes_hex(&source_bytes_after),
                    "disk_source_after_sha256": format!("{:x}", Sha256::digest(&source_bytes_after)),
                    "disk_source_unchanged": source_bytes_before == source_bytes_after,
                },
                "syntax_issues":syntax.issues.iter().map(|issue| json!({"message":issue.message,"start":issue.start,"end":issue.end})).collect::<Vec<_>>() }));
            assert_eq!(source_bytes_after, source.as_bytes());
        }
    }
    if let Some(destination) = std::env::var_os("MAMBA_QUALITY_ISORT_EXCLUDED_EVIDENCE") {
        fs::write(
            destination,
            serde_json::to_vec_pretty(&json!({
                "schema":"mamba.quality.isort-excluded-comparison.v1",
                "corpus_sha256":format!("{:x}",Sha256::digest(corpus.as_bytes())),
                "cases":results,"failed":failures,"whole_suite_accepted":false,
            }))
            .unwrap(),
        )
        .unwrap();
    }
    assert_eq!(
        failures, 0,
        "Pinned recovered isort API or checker comparisons failed"
    );
}

fn version_to_target(version: PythonVersion) -> quality_syntax::PythonTarget {
    match version {
        PythonVersion::Py312 => quality_syntax::PythonTarget::Py312,
        PythonVersion::Py313 => quality_syntax::PythonTarget::Py313,
        PythonVersion::Py314 => quality_syntax::PythonTarget::Py314,
    }
}

fn bytes_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
