//! Both semantic platforms have captured originals for all three targets.
use crate::domain::{model::CompletionStatus, ports::ProjectStore, source::PythonVersion};
use crate::infrastructure::filesystem::Filesystem;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs;

#[test]
fn importlib_child_visibility_matches_captured_originals() {
    let cases = [
        ("explicit_child", "import importlib.util\n\nspec = importlib.util.spec_from_file_location(\"probe\", \"probe.py\")\n", "38785816acda38ef581991da61ea64122300a0a88d25c46ca8951bf3cf19548d", None),
        ("aliased_child", "import importlib.util as util\n\nspec = util.spec_from_file_location(\"probe\", \"probe.py\")\n", "66b4453508c912ea1f2f3a36c26cefcdcfd3cb9bc36fe979cec8aa1975007c25", None),
        ("bare_parent", "import importlib\n\nmodule = importlib.util\n", "780bc2ad4c0c72b9846b9eb5c9a552807c6d23417cbe8ec62f7b30bb4c391cbb", Some("Module has no attribute \"util\"")),
        ("unknown_member", "import importlib.util\n\nvalue = importlib.util.definitely_missing_mamba_gap_member\n", "9c011fbbc63e2ea8498f43659edbd7bd0ad9ed9588f4d32cec3a15cbff6b6fda", Some("Module has no attribute \"definitely_missing_mamba_gap_member\"")),
    ];
    for version in [
        PythonVersion::Py312,
        PythonVersion::Py313,
        PythonVersion::Py314,
    ] {
        for (name, source, hash, expected_message) in cases {
            assert_eq!(format!("{:x}", Sha256::digest(source.as_bytes())), hash);
            let root = tempfile::tempdir().unwrap();
            let path = fs::canonicalize(root.path()).unwrap();
            fs::write(path.join("pyproject.toml"), format!(
                "[project]\nname = \"fixture\"\nversion = \"0.0.0\"\nrequires-python = \">={}\"\n", version.name()
            )).unwrap();
            let file = path.join("main.py");
            fs::write(&file, source).unwrap();
            let workspace = Filesystem.load(std::slice::from_ref(&file), true);
            let report = super::super::check(&workspace);
            assert_eq!(report.completion, CompletionStatus::Incomplete);
            assert_eq!(report.reference_exit_code, None);
            assert_eq!(report.checked_files, 1);
            assert_eq!(fs::read_to_string(file).unwrap(), source);
            let messages: Vec<_> = report
                .diagnostics
                .iter()
                .map(|d| d.message.as_str())
                .collect();
            assert_eq!(
                messages,
                expected_message.into_iter().collect::<Vec<_>>(),
                "{name} {}",
                version.name()
            );
            for diagnostic in &report.diagnostics {
                let value = serde_json::to_value(diagnostic).unwrap();
                assert_eq!(value["rule"], "attr-defined");
                assert_eq!(value["location"]["line"], 3);
            }
        }
    }
}

// Exact diagnostics from 48 qualified originals: four cases, three targets,
// two semantic platforms, and two repeats. Native checks run once per target.
#[test]
fn pinned_original_typed_importlib_controls() {
    let cases = [
        ("alias_wrong_argument", "import importlib.util as util\n\nspec = util.spec_from_file_location(123, \"probe.py\")\n", "5b1aa5b7558c85cf39934fd9df12cae02826e7522f5e9894c9f106019ff98aa3", Some(("arg-type", "Argument 1 to \"spec_from_file_location\" has incompatible type \"int\"; expected \"str\""))),
        ("alias_wrong_assignment", "import importlib.util as util\n\nvalue: str = util.spec_from_file_location(\"probe\", \"probe.py\")\n", "ae088ee8eb7b1295461083f0dccd50e68fc5bdab3b1c406c9ead34560123a21d", Some(("assignment", "Incompatible types in assignment (expression has type \"ModuleSpec | None\", variable has type \"str\")"))),
        ("dotted_none_spec", "import importlib.util\n\nmodule = importlib.util.module_from_spec(None)\n", "9ac9f8013f6a44cd4b167344fb3d29ba267ae16e64722a7027f43a4a1d071e75", Some(("arg-type", "Argument 1 to \"module_from_spec\" has incompatible type \"None\"; expected \"ModuleSpec\""))),
        ("valid_narrowed_spec", "import importlib.util\n\nspec = importlib.util.spec_from_file_location(\"probe\", \"probe.py\")\nif spec is not None:\n    module = importlib.util.module_from_spec(spec)\n", "44931b54ea9307e2eba883e1ef21dca979bd6a1eeda5167b011d6061a843194a", None),
    ];
    for version in [
        PythonVersion::Py312,
        PythonVersion::Py313,
        PythonVersion::Py314,
    ] {
        for (name, source, hash, expected) in cases {
            assert_eq!(format!("{:x}", Sha256::digest(source.as_bytes())), hash);
            let root = tempfile::tempdir().unwrap();
            let path = fs::canonicalize(root.path()).unwrap();
            fs::write(path.join("pyproject.toml"), format!(
                "[project]\nname = \"fixture\"\nversion = \"0.0.0\"\nrequires-python = \">={}\"\n", version.name()
            )).unwrap();
            let file = path.join("main.py");
            fs::write(&file, source).unwrap();
            let workspace = Filesystem.load(std::slice::from_ref(&file), true);
            let report = super::super::check(&workspace);
            assert_eq!(report.completion, CompletionStatus::Incomplete);
            assert_eq!(report.reference_exit_code, None);
            assert_eq!(report.checked_files, 1);
            assert_eq!(fs::read(&file).unwrap(), source.as_bytes());
            assert_eq!(
                format!("{:x}", Sha256::digest(fs::read(&file).unwrap())),
                hash
            );
            let actual: Vec<_> = report
                .diagnostics
                .iter()
                .map(|diagnostic| {
                    let value = serde_json::to_value(diagnostic).unwrap();
                    (
                        value["rule"].as_str().unwrap().to_string(),
                        diagnostic.message.clone(),
                        value["location"]["line"].as_u64().unwrap(),
                    )
                })
                .collect();
            let expected: Vec<_> = expected
                .into_iter()
                .map(|(rule, message)| (rule.to_string(), message.to_string(), 3_u64))
                .collect();
            assert_eq!(actual, expected, "{name} {}", version.name());
        }
    }
}

#[test]
fn metadata_synthetic_parent_keeps_typed_util_visible() {
    let source = r###"import importlib.util
from types import ModuleType

def unsafe(path: str) -> ModuleType:
    spec = importlib.util.spec_from_file_location("fixture", path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module

def safe(path: str) -> ModuleType:
    spec = importlib.util.spec_from_file_location("fixture", path)
    if spec is None or spec.loader is None:
        raise RuntimeError("missing loader")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module
"###;
    let original = r###"{"file": "main.py", "line": 6, "column": 45, "end_line": 6, "end_column": 49, "message": "Argument 1 to \"module_from_spec\" has incompatible type \"ModuleSpec | None\"; expected \"ModuleSpec\"", "hint": null, "code": "arg-type", "severity": "error"}
{"file": "main.py", "line": 7, "column": 4, "end_line": 7, "end_column": 15, "message": "Item \"None\" of \"ModuleSpec | None\" has no attribute \"loader\"", "hint": null, "code": "union-attr", "severity": "error"}
{"file": "main.py", "line": 7, "column": 4, "end_line": 7, "end_column": 27, "message": "Item \"None\" of \"Loader | Any | None\" has no attribute \"exec_module\"", "hint": null, "code": "union-attr", "severity": "error"}
"###;
    assert_eq!(
        format!("{:x}", Sha256::digest(source.as_bytes())),
        "12b695411e70cddbf3a1be87e25f73cbe1b68408e8c5dcc2b3b0610df3adbc14"
    );
    assert_eq!(
        format!("{:x}", Sha256::digest(original.as_bytes())),
        "e89e36aa97676d43882a34023b484d5acd3e22e952fa6047ba225fef4ca65b2c"
    );
    let cohort_source = r###"import importlib.metadata
"###;
    assert_eq!(
        format!("{:x}", Sha256::digest(cohort_source.as_bytes())),
        "be4048a827586d21c6dd3ec5a6ec7d8f2c3499c2027a4fff3cd85b895d0c5a85"
    );
    let expected: Vec<Value> = original
        .lines()
        .map(|line| {
            let row: Value = serde_json::from_str(line).unwrap();
            json!({"rule": row["code"], "message": row["message"], "hint": row["hint"],
                "path": row["file"], "severity": row["severity"],
                "location": {"line": row["line"], "column": row["column"].as_i64().unwrap() + 1,
                    "end_line": row["end_line"], "end_column": row["end_column"]}})
        })
        .collect();
    for target in [
        PythonVersion::Py312,
        PythonVersion::Py313,
        PythonVersion::Py314,
    ] {
        let root = tempfile::tempdir().unwrap();
        let root_path = fs::canonicalize(root.path()).unwrap();
        fs::write(
            root_path.join("pyproject.toml"),
            "[project]\nname = \"fixture\"\nversion = \"0.0.0\"\nrequires-python = \">=3.12\"\n",
        )
        .unwrap();
        let path = root_path.join("main.py");
        fs::write(&path, source).unwrap();
        let cohort_path = root_path.join("cohort.py");
        fs::write(&cohort_path, cohort_source).unwrap();
        let mut workspace = Filesystem.load(&[path.clone(), cohort_path.clone()], true);
        workspace.projects[0].python_version = target;
        let report = super::super::check(&workspace);
        assert_eq!(report.completion, CompletionStatus::Incomplete);
        assert_eq!(report.reference_exit_code, None);
        assert_eq!(report.checked_files, 2);
        let actual: Vec<_> = report.diagnostics.iter().map(|diagnostic| json!({
            "rule": diagnostic.rule, "message": diagnostic.message, "hint": diagnostic.hint,
            "path": diagnostic.path.strip_prefix(&root_path).unwrap(),
            "severity": diagnostic.severity,
            "location": {"line": diagnostic.location.line, "column": diagnostic.location.column,
                "end_line": diagnostic.location.end_line, "end_column": diagnostic.location.end_column},
        })).collect();
        assert_eq!(actual, expected, "{}", target.name());
        assert_eq!(fs::read(&path).unwrap(), source.as_bytes());
        assert_eq!(fs::read(&cohort_path).unwrap(), cohort_source.as_bytes());
    }
}
