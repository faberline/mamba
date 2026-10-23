//! Fixed original controls exercise rules together in the same source file.
use super::*;

#[test]
fn pinned_original_rule_interactions() {
    let corpus = include_str!("interactions.json");
    assert_eq!(
        format!("{:x}", Sha256::digest(corpus.as_bytes())),
        "a3d1af9e2d2aa79dc4b856d8f9d17f812619f113d03bd210a1cee41a4bb062ec"
    );
    let data: Value = serde_json::from_str(corpus).unwrap();
    let sources = data["sources"].as_object().unwrap();
    assert_eq!(sources.len(), 29);
    let profile = FixedProfile::locked();
    let mut results = Vec::new();
    let mut failures = 0;
    for (target, version) in [
        PythonVersion::Py312,
        PythonVersion::Py313,
        PythonVersion::Py314,
    ]
    .into_iter()
    .enumerate()
    {
        let root = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(root.path()).unwrap();
        fs::write(
            root.join("pyproject.toml"),
            "[project]\nrequires-python='>=3.12'\n",
        )
        .unwrap();
        for (name, source) in sources {
            let raw = bytes(&source["source_hex"]);
            assert_eq!(
                format!("{:x}", Sha256::digest(&raw)),
                source["source_sha256"].as_str().unwrap()
            );
            fs::write(root.join(name), raw).unwrap();
        }
        let paths: Vec<_> = sources.keys().map(|name| root.join(name)).collect();
        let expected = &data["versions"][target];
        let original = workspace(&paths, version);
        assert_eq!(original.sources.len(), 29);
        let before = check(&original, &profile);
        let before_actual = reports::diagnostics(&before, &root);
        let before_equal = before_actual == expected["before"]
            && before.reference_exit_code == expected["before_exit"].as_i64().map(|v| v as i32)
            && before.completion == CompletionStatus::Complete;
        failures += usize::from(!before_equal);
        let fixed = transform(&original, &profile);
        assert!(
            fixed.issues.is_empty(),
            "Unexpected transformation errors: {:?}",
            fixed.issues
        );
        assert_eq!(fixed.completion, CompletionStatus::Complete);
        let mut outputs: Vec<_> = original
            .sources
            .iter()
            .map(|source| source.bytes.clone())
            .collect();
        for (index, candidate) in &fixed.candidates {
            outputs[*index] = Filesystem
                .candidate_bytes(&original.sources[*index], candidate)
                .unwrap();
        }
        let mut files = Vec::new();
        for (source, output) in original.sources.iter().zip(&outputs) {
            let name = source.path.strip_prefix(&root).unwrap().to_str().unwrap();
            let expected_bytes = bytes(&expected["fixed_hex"][name]);
            let equal = *output == expected_bytes;
            failures += usize::from(!equal);
            files.push(json!({"path":name,"equal":equal,
                "expected_sha256":format!("{:x}",Sha256::digest(&expected_bytes)),
                "actual_sha256":format!("{:x}",Sha256::digest(output))}));
            fs::write(&source.path, output).unwrap();
        }
        let fixed_workspace = workspace(&paths, version);
        let after = check(&fixed_workspace, &profile);
        let after_actual = reports::diagnostics(&after, &root);
        let after_equal = after_actual == expected["after"]
            && after.reference_exit_code == expected["after_exit"].as_i64().map(|v| v as i32)
            && after.completion == CompletionStatus::Complete;
        failures += usize::from(!after_equal);
        let repeated = transform(&fixed_workspace, &profile);
        let stable = repeated.candidates.is_empty()
            && repeated.issues.is_empty()
            && repeated.completion == CompletionStatus::Complete;
        failures += usize::from(!stable);
        results.push(json!({"python":version.name(),"before_equal":before_equal,
            "before_expected":expected["before"],"before_actual":before_actual,
            "before_exit":before.reference_exit_code,"after_equal":after_equal,
            "after_expected":expected["after"],"after_actual":after_actual,
            "after_exit":after.reference_exit_code,"files":files,"stable":stable}));
    }
    if let Some(destination) = std::env::var_os("MAMBA_QUALITY_RUFF_INTERACTION_EVIDENCE") {
        fs::write(
            destination,
            serde_json::to_vec_pretty(&json!({
                "schema":"mamba.quality.ruff-interaction-comparison.v1",
                "corpus_sha256":format!("{:x}",Sha256::digest(corpus.as_bytes())),
                "versions":results,"failed":failures,"whole_suite_accepted":false,
            }))
            .unwrap(),
        )
        .unwrap();
    }
    assert_eq!(failures, 0, "Pinned Ruff interaction comparisons failed");
}
