//! Imported functions retain callable identity when assigned to module names.
use crate::domain::{model::CompletionStatus, ports::ProjectStore, source::PythonVersion};
use crate::infrastructure::filesystem::Filesystem;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs;

const CORPUS: &str = include_str!("imported_function_aliases.json");

fn decode_hex(encoded: &str) -> Vec<u8> {
    encoded
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let digit = |value: u8| match value {
                b'0'..=b'9' => value - b'0',
                b'a'..=b'f' => value - b'a' + 10,
                b'A'..=b'F' => value - b'A' + 10,
                _ => panic!("Invalid hexadecimal fixture byte: {value}"),
            };
            (digit(pair[0]) << 4) | digit(pair[1])
        })
        .collect()
}

#[test]
fn pinned_imported_function_alias_naming() {
    let cases: Vec<Value> = serde_json::from_str(CORPUS).unwrap();
    assert_eq!(cases.len(), 12);
    let mut results = Vec::new();
    let mut failures = 0;

    for case in cases {
        let source = decode_hex(case["source_hex"].as_str().unwrap());
        assert_eq!(
            format!("{:x}", Sha256::digest(&source)),
            case["source_sha256"].as_str().unwrap()
        );
        let directory = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(directory.path()).unwrap();
        fs::write(
            root.join("pyproject.toml"),
            "[project]\nrequires-python = \">=3.12\"\n",
        )
        .unwrap();
        fs::write(root.join("main.py"), &source).unwrap();

        let mut workspace = Filesystem.load(std::slice::from_ref(&root), true);
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
            assert_eq!(report.checked_files, 1);

            let actual: Vec<_> = report
                .diagnostics
                .iter()
                .map(|diagnostic| {
                    json!({
                        "rule": diagnostic.rule,
                        "message": diagnostic.message,
                        "hint": diagnostic.hint,
                        "path": diagnostic.path.strip_prefix(&root).unwrap(),
                        "severity": diagnostic.severity,
                        "location": {
                            "line": diagnostic.location.line,
                            "column": diagnostic.location.column,
                            "end_line": diagnostic.location.end_line,
                            "end_column": diagnostic.location.end_column,
                        },
                    })
                })
                .collect();
            let exit = report.diagnostics.iter().fold(0, |code, diagnostic| {
                code | match diagnostic.severity.as_str() {
                    "fatal" => 1,
                    "error" => 2,
                    "warning" => 4,
                    "refactor" => 8,
                    "convention" => 16,
                    "info" => 0,
                    other => panic!("Unexpected Pylint severity: {other}"),
                }
            });
            let expected = case["expected"][target].clone();
            let reference_exit = case["reference_exits"][target].as_i64().unwrap();
            let equal = Value::Array(actual.clone()) == expected && exit == reference_exit;
            failures += usize::from(!equal);
            results.push(json!({
                "name": case["name"],
                "python": version.name(),
                "expected": expected,
                "actual": actual,
                "reference_exit": reference_exit,
                "diagnostic_exit": exit,
                "equal": equal,
            }));
            assert_eq!(fs::read(root.join("main.py")).unwrap(), source);
        }
    }

    if let Some(destination) =
        std::env::var_os("MAMBA_QUALITY_PYLINT_IMPORTED_FUNCTION_ALIASES_EVIDENCE")
    {
        fs::write(
            destination,
            serde_json::to_vec_pretty(&json!({
                "schema": "mamba.quality.pylint-imported-function-alias-comparison.v1",
                "corpus_sha256": format!("{:x}", Sha256::digest(CORPUS.as_bytes())),
                "cases": results,
                "failed": failures,
                "whole_suite_accepted": false,
            }))
            .unwrap(),
        )
        .unwrap();
    }
    assert_eq!(failures, 0, "Imported-function alias naming differs");
}

#[test]
fn resolves_sixty_five_imported_function_aliases() {
    let mut source =
        String::from("\"\"\"Alias-depth control.\"\"\"\nimport sys\nalias_0 = sys.exit\n");
    for index in 1..65 {
        source.push_str(&format!("alias_{index} = alias_{}\n", index - 1));
    }
    source.push_str("SINE = alias_64\n");
    let directory = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(directory.path()).unwrap();
    fs::write(
        root.join("pyproject.toml"),
        "[project]\nrequires-python = \">=3.12\"\n",
    )
    .unwrap();
    fs::write(root.join("main.py"), &source).unwrap();
    let workspace = Filesystem.load(std::slice::from_ref(&root), true);
    let report = super::super::check(&workspace);
    let invalid_names: Vec<_> = report
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.rule == "C0103")
        .map(|diagnostic| diagnostic.message.as_str())
        .collect();
    assert_eq!(
        invalid_names,
        ["Variable name \"SINE\" doesn't conform to snake_case naming style"]
    );
    assert_eq!(report.completion, CompletionStatus::Incomplete);
    assert_eq!(report.reference_exit_code, None);
    assert_eq!(fs::read(root.join("main.py")).unwrap(), source.as_bytes());
}
