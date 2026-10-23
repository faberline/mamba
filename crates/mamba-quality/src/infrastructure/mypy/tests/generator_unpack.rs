//! Qualified Darwin originals retain exact diagnostics and source bytes.
use crate::domain::{model::CompletionStatus, ports::ProjectStore, source::PythonVersion};
use crate::infrastructure::filesystem::Filesystem;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs;

const CASES: &str = r#"[
{"name": "typed_pair", "source": "def probe(resources: dict[str, int]) -> None:\n    parent, child = (resources[role] for role in (\"parent\", \"child\"))\n    parent_ok: int = parent\n    child_ok: int = child\n", "source_sha256": "d851047b90c0ab7cc9d967e630abd643fba82f9ade5a1627603620fc8c385ac5", "targets": {"3.12": {"original_cli_exit": 0, "original_diagnostics_verbatim": []}, "3.13": {"original_cli_exit": 0, "original_diagnostics_verbatim": []}, "3.14": {"original_cli_exit": 0, "original_diagnostics_verbatim": []}}},
{"name": "wrong_assignment", "source": "def probe(resources: dict[str, int]) -> None:\n    parent, child = (resources[role] for role in (\"parent\", \"child\"))\n    wrong: str = parent\n    child_ok: int = child\n", "source_sha256": "07b2d05baf80046450ef8142422a15d63a0ce2d8c5ce7209f707d40f8c9737ef", "targets": {"3.12": {"original_cli_exit": 1, "original_diagnostics_verbatim": [{"code": "assignment", "column": 17, "end_column": 23, "end_line": 3, "file": "main.py", "hint": null, "line": 3, "message": "Incompatible types in assignment (expression has type \"int\", variable has type \"str\")", "severity": "error"}]}, "3.13": {"original_cli_exit": 1, "original_diagnostics_verbatim": [{"code": "assignment", "column": 17, "end_column": 23, "end_line": 3, "file": "main.py", "hint": null, "line": 3, "message": "Incompatible types in assignment (expression has type \"int\", variable has type \"str\")", "severity": "error"}]}, "3.14": {"original_cli_exit": 1, "original_diagnostics_verbatim": [{"code": "assignment", "column": 17, "end_column": 23, "end_line": 3, "file": "main.py", "hint": null, "line": 3, "message": "Incompatible types in assignment (expression has type \"int\", variable has type \"str\")", "severity": "error"}]}}},
{"name": "starred_unpack", "source": "def probe() -> None:\n    first, *middle, last = (value for value in (1, 2, 3, 4))\n    first_ok: int = first\n    middle_ok: list[int] = middle\n    last_ok: int = last\n", "source_sha256": "2789161fb45cdf87a6b650fa013d2e5983e7c94a514e2ca54df08d74238bafeb", "targets": {"3.12": {"original_cli_exit": 0, "original_diagnostics_verbatim": []}, "3.13": {"original_cli_exit": 0, "original_diagnostics_verbatim": []}, "3.14": {"original_cli_exit": 0, "original_diagnostics_verbatim": []}}},
{"name": "any_input_names", "source": "from typing import Any\n\ndef probe(prompt: Any) -> None:\n    parent, child = (prompt[\"resources\"][role] for role in (\"parent\", \"child\"))\n    parent_path = parent[\"path\"]\n    child_path = child[\"path\"]\n", "source_sha256": "ee23e39cd06fd8d70a9884fc6c087f0acffb363bb98c7e2faff64265fc7f8301", "targets": {"3.12": {"original_cli_exit": 0, "original_diagnostics_verbatim": []}, "3.13": {"original_cli_exit": 0, "original_diagnostics_verbatim": []}, "3.14": {"original_cli_exit": 0, "original_diagnostics_verbatim": []}}}
]"#;

#[test]
fn pinned_original_generator_unpack() {
    assert_eq!(
        format!("{:x}", Sha256::digest(CASES.as_bytes())),
        "6a2f6018d0b348f49b83afa11728e6ed865c597e24ecfa27a72e0509b251624c"
    );
    let cases: Vec<Value> = serde_json::from_str(CASES).unwrap();
    assert_eq!(cases.len(), 4);
    let mut checked = 0;
    for case in cases {
        let source = case["source"].as_str().unwrap();
        let source_hash = case["source_sha256"].as_str().unwrap();
        assert_eq!(
            format!("{:x}", Sha256::digest(source.as_bytes())),
            source_hash
        );
        let root = tempfile::tempdir().unwrap();
        let root_path = fs::canonicalize(root.path()).unwrap();
        let file = root_path.join("main.py");
        fs::write(&file, source).unwrap();
        let mut workspace = Filesystem.load(std::slice::from_ref(&file), true);
        assert_eq!(workspace.sources.len(), 1);
        for version in [
            PythonVersion::Py312,
            PythonVersion::Py313,
            PythonVersion::Py314,
        ] {
            workspace.projects[0].python_version = version;
            let original = &case["targets"][version.name()];
            let raw = original["original_diagnostics_verbatim"]
                .as_array()
                .unwrap();
            // Keep Mypy corpus mapping: zero-based start + 1; original end unchanged.
            let expected: Vec<_> = raw.iter().map(|message| json!({
                "rule": message["code"], "message": message["message"], "hint": message["hint"],
                "path": message["file"], "severity": message["severity"],
                "location": {"line": message["line"], "column": message["column"].as_u64().unwrap() + 1,
                    "end_line": message["end_line"], "end_column": message["end_column"]},
            })).collect();
            let report = super::super::check(&workspace);
            assert_eq!(report.completion, CompletionStatus::Incomplete);
            assert_eq!(report.reference_exit_code, None);
            assert_eq!(report.checked_files, 1);
            assert_eq!(fs::read(&file).unwrap(), source.as_bytes());
            assert_eq!(
                format!("{:x}", Sha256::digest(fs::read(&file).unwrap())),
                source_hash
            );
            let actual: Vec<_> = report.diagnostics.iter().map(|diagnostic| json!({
                "rule": diagnostic.rule, "message": diagnostic.message, "hint": diagnostic.hint,
                "path": diagnostic.path.strip_prefix(&root_path).unwrap(), "severity": diagnostic.severity,
                "location": {"line": diagnostic.location.line, "column": diagnostic.location.column,
                    "end_line": diagnostic.location.end_line, "end_column": diagnostic.location.end_column},
            })).collect();
            assert_eq!(actual, expected, "{} {}", case["name"], version.name());
            let diagnostic_exit = i64::from(
                report
                    .diagnostics
                    .iter()
                    .any(|diagnostic| diagnostic.severity == "error"),
            );
            assert_eq!(
                Some(diagnostic_exit),
                original["original_cli_exit"].as_i64()
            );
            assert!(report
                .diagnostics
                .iter()
                .all(|diagnostic| diagnostic.fix.is_none()));
            checked += 1;
        }
    }
    assert_eq!(checked, 12);
}
