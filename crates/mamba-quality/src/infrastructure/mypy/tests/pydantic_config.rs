//! Pinned Pydantic configuration diagnostics retain all original fields.
use crate::domain::{model::CompletionStatus, ports::ProjectStore, source::PythonVersion};
use crate::infrastructure::filesystem::Filesystem;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs;

#[test]
fn pinned_original_pydantic_config() {
    let corpus = include_str!("pydantic_config.json");
    let cases: Vec<Value> = serde_json::from_str(corpus).unwrap();
    // These seven original invocations selected main.py and followed local imports.
    let entrypoints: Value =
        serde_json::from_str(include_str!("pydantic_config_entrypoints.json")).unwrap();
    assert_eq!(entrypoints.as_object().unwrap().len(), 7);
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
        let selected = entrypoints.get(case["name"].as_str().unwrap());
        if let Some(selected) = selected {
            assert_eq!(selected["source_sha256"], case["source_sha256"]);
        }
        let paths = selected.map_or_else(
            || vec![root_path.clone()],
            |selected| {
                selected["entrypoints"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|path| root_path.join(path.as_str().unwrap()))
                    .collect()
            },
        );
        let selected_files = selected.map_or(files.len(), |selected| {
            selected["entrypoints"].as_array().unwrap().len()
        });
        let mut workspace = Filesystem.load(&paths, true);
        assert_eq!(workspace.sources.len(), selected_files);
        for (target, version) in [
            PythonVersion::Py312,
            PythonVersion::Py313,
            PythonVersion::Py314,
        ]
        .into_iter()
        .enumerate()
        {
            workspace.projects[0].python_version = version;
            let disk_hashes = || {
                files
                    .keys()
                    .map(|name| {
                        let bytes = fs::read(root_path.join(name)).unwrap();
                        (name.clone(), json!(format!("{:x}", Sha256::digest(bytes))))
                    })
                    .collect::<serde_json::Map<String, Value>>()
            };
            let disk_before = disk_hashes();
            let loaded_sources: Vec<_> = workspace
                .sources
                .iter()
                .map(|source| {
                    json!({
                        "path": source.path.strip_prefix(&root_path).unwrap(),
                        "snapshot_sha256": source.sha256,
                        "bytes_sha256": format!("{:x}", Sha256::digest(&source.bytes)),
                        "text_sha256": format!("{:x}", Sha256::digest(source.text.as_bytes())),
                    })
                })
                .collect();
            let report = super::super::check(&workspace);
            let disk_after = disk_hashes();
            assert_eq!(report.completion, CompletionStatus::Incomplete);
            assert_eq!(report.reference_exit_code, None);
            assert_eq!(report.checked_files, selected_files);
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
            let equal = actual == *expected
                && case["reference_exits"][target].as_i64() == Some(i64::from(diagnostic_exit));
            failures += usize::from(!equal);
            results.push(json!({
                "name":case["name"],"files":files,"python":version.name(),
                "expected":expected,"actual":actual,"equal":equal,
                "reference_exit":case["reference_exits"][target],"diagnostic_exit":diagnostic_exit,
                "selected_entrypoints":selected.map(|selected| &selected["entrypoints"]),
                "checked_files":report.checked_files,
                "actual_tool_report":report,
                "input_root":root_path,"requested_paths":paths,
                "loaded_sources":loaded_sources,"disk_before":disk_before,"disk_after":disk_after,
            }));
            for (filename, contents) in files {
                assert_eq!(
                    fs::read(root_path.join(filename)).unwrap(),
                    contents.as_str().unwrap().as_bytes()
                );
            }
        }
    }
    if let Some(destination) = std::env::var_os("MAMBA_QUALITY_MYPY_CONFIG_EVIDENCE") {
        fs::write(
            destination,
            serde_json::to_vec_pretty(&json!({
                "schema":"mamba.quality.mypy-pydantic-config-comparison.v1",
                "corpus_sha256":format!("{:x}",Sha256::digest(corpus.as_bytes())),
                "cases":results,"failed":failures,"whole_suite_accepted":false,
            }))
            .unwrap(),
        )
        .unwrap();
    }
    assert_eq!(
        failures, 0,
        "Pinned Pydantic configuration comparisons failed"
    );
}
