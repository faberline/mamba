//! Only proved older-target AST addresses may differ from the pinned original.
use super::super::*;
use crate::domain::{ports::ProjectStore, source::PythonVersion};
use crate::infrastructure::filesystem::Filesystem;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs;

fn address_message(message: &str) -> Option<String> {
    let (before, after) = message.split_once(" object at 0x")?;
    if !["Name", "Lambda", "Starred", "UnaryOp"]
        .iter()
        .any(|kind| before.ends_with(&format!("<ast.{kind}")))
    {
        return None;
    }
    let address = after.strip_suffix(">.")?;
    if address.is_empty() || !address.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    Some(format!("{before} object at 0xADDRESS>."))
}

#[test]
fn pinned_original_ast_errors() {
    let corpus = include_str!("ast_errors.json");
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
            let actual = match format(&workspace.sources[0], &workspace, &FixedProfile::locked()) {
                Ok((output, remaining)) => json!({"output":output,"remaining":remaining}),
                Err(error) => json!({"error_class":error.code,"message":error.message,
                    "reference_exit":if error.code == "ValueError" {0} else {1}}),
            };
            let message_equal = actual["message"].as_str().is_some_and(|message| {
                let expected = expected["message"].as_str().unwrap();
                if version == PythonVersion::Py314 {
                    message == expected
                } else {
                    address_message(message).is_some()
                        && address_message(message) == address_message(expected)
                }
            });
            let equal = actual["error_class"] == expected["error_class"]
                && actual["reference_exit"] == expected["reference_exit"]
                && message_equal;
            failures += usize::from(!equal);
            results.push(json!({"name":case["name"],"python":version.name(),
                "expected":expected,"actual":actual,"equal":equal,
                "allowed_variant":if version == PythonVersion::Py314 {"none"} else {"proved AST process address only"}}));
            assert_eq!(fs::read(&path).unwrap(), source.as_bytes());
        }
    }
    if let Some(destination) = std::env::var_os("MAMBA_QUALITY_ISORT_AST_ERRORS_EVIDENCE") {
        fs::write(
            destination,
            serde_json::to_vec_pretty(&json!({
                "schema":"mamba.quality.isort-ast-error-comparison.v1",
                "corpus_sha256":format!("{:x}",Sha256::digest(corpus.as_bytes())),
                "cases":results,"failed":failures,"whole_suite_accepted":false,
            }))
            .unwrap(),
        )
        .unwrap();
    }
    assert_eq!(failures, 0, "Pinned isort AST failure comparisons failed");
}
