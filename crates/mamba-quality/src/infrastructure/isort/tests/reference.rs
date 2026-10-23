//! Fixed-profile original-tool comparison. Sources and outputs are frozen copies.
use super::super::*;
use crate::domain::source::PythonVersion;
use crate::{domain::ports::ProjectStore, infrastructure::filesystem::Filesystem};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs;

#[test]
fn pinned_original_corpus() {
    let cases: Vec<Value> = super::catalog::CHUNKS
        .iter()
        .flat_map(|chunk| serde_json::from_str::<Vec<Value>>(chunk).unwrap())
        .collect();
    assert_eq!(cases.len(), super::catalog::CASE_COUNT);
    let root = tempfile::tempdir().unwrap();
    let profile = FixedProfile::locked();
    let mut results = Vec::new();
    let mut failures = 0;
    for (case_index, case) in cases.into_iter().enumerate() {
        let case_root = root.path().join(case_index.to_string());
        fs::create_dir(&case_root).unwrap();
        if let Some(directories) = case["directories"].as_array() {
            for directory in directories {
                fs::create_dir_all(case_root.join(directory.as_str().unwrap())).unwrap();
            }
        }
        if let Some(files) = case["files"].as_object() {
            for (name, contents) in files {
                let path = case_root.join(name);
                fs::create_dir_all(path.parent().unwrap()).unwrap();
                fs::write(path, contents.as_str().unwrap()).unwrap();
            }
        }
        let path = case_root.join(format!(
            "main.{}",
            case["extension"].as_str().unwrap_or("py")
        ));
        fs::write(&path, "").unwrap();
        let mut workspace = Filesystem.load(std::slice::from_ref(&path), true);
        let source = case["source"].as_str().unwrap();
        let id = case["id"].as_str().unwrap();
        assert_eq!(
            format!("{:x}", Sha256::digest(source.as_bytes())),
            case["source_sha256"].as_str().unwrap_or(id)
        );
        for (index, version) in [
            PythonVersion::Py312,
            PythonVersion::Py313,
            PythonVersion::Py314,
        ]
        .into_iter()
        .enumerate()
        {
            workspace.projects[0].python_version = version;
            workspace.sources[0].text = source.into();
            let expected = &case["expected"][index];
            let result = format(&workspace.sources[0], &workspace, &profile);
            let (output, remaining, error) = match result {
                Ok((output, remaining)) => (Some(output), remaining, None),
                Err(error) => (
                    None,
                    Vec::new(),
                    Some(format!("{}: {}", error.code, error.message)),
                ),
            };
            let equal = if expected.get("reference_error").is_some() {
                error.as_deref() == expected["reference_error"].as_str()
            } else {
                output.as_deref() == expected["output"].as_str() && remaining.is_empty()
            };
            let mut converges = true;
            let mut repeated = None;
            if equal && expected["reference_converges"] == true {
                if let Some(output) = &output {
                    workspace.sources[0].text = output.clone();
                    if let Ok((second, remaining)) =
                        format(&workspace.sources[0], &workspace, &profile)
                    {
                        converges = second == *output && remaining.is_empty();
                        repeated = Some(second);
                    } else {
                        converges = false;
                    }
                }
            }
            if !equal || !converges {
                failures += 1;
            }
            results.push(json!({"id": id, "python": version.name(), "source": source,
                "origins": case["origins"], "files": case["files"], "directories": case["directories"],
                "expected": expected, "actual": output,
                "remaining": remaining, "error": error, "equal": equal, "converges": converges, "repeated": repeated}));
        }
    }
    if let Some(destination) = std::env::var_os("MAMBA_QUALITY_ISORT_EVIDENCE") {
        let report = json!({"schema": "mamba.quality.isort-comparison.v1", "cases": results,
            "corpus_sha256": super::catalog::CORPUS_SHA256, "exception_message_parity": true,
            "failed": failures, "equal": results.len() - failures,
            "whole_suite_accepted": false});
        fs::write(destination, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
    }
    assert_eq!(failures, 0, "Pinned isort byte comparisons failed");
}
