//! Focused public-checker controls for native `ast.Constant` lookup and calls.
use crate::domain::{ports::ProjectStore, source::PythonVersion};
use crate::infrastructure::filesystem::Filesystem;
use std::{fs, path::PathBuf};

fn check(inputs: &[PathBuf], target: PythonVersion) -> Vec<(String, String)> {
    let mut workspace = Filesystem.load(inputs, true);
    for project in &mut workspace.projects {
        project.python_version = target;
    }
    super::super::check(&workspace)
        .diagnostics
        .into_iter()
        .map(|diagnostic| {
            let row = serde_json::to_value(&diagnostic).unwrap();
            (row["rule"].as_str().unwrap().to_owned(), diagnostic.message)
        })
        .collect()
}

fn fixture(source: &str) -> (tempfile::TempDir, PathBuf) {
    let temp = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(temp.path()).unwrap();
    let path = root.join("main.py");
    fs::write(&path, source).unwrap();
    (temp, path)
}

#[test]
fn both_ast_import_forms_match_the_original_diagnostic_on_all_targets() {
    let expected = vec![(
        "str-bytes-safe".into(),
        "If x = b'abc' then f\"{x}\" or \"{}\".format(x) produces \"b'abc'\", not \"abc\". If this is desired behavior, use f\"{x!r}\" or \"{!r}\".format(x). Otherwise, decode the bytes".into(),
    )];
    for source in [
        "from ast import Constant\ndef render(value: Constant) -> str:\n    return f\"{value.value}\"\n",
        "import ast\ndef render(value: ast.Constant) -> str:\n    return f\"{value.value}\"\n",
    ] {
        let (_temp, path) = fixture(source);
        for target in [PythonVersion::Py312, PythonVersion::Py313, PythonVersion::Py314] {
            assert_eq!(check(std::slice::from_ref(&path), target), expected);
        }
    }
}

#[test]
fn constructor_position_keys_are_typed_closed_and_value_can_be_keyworded() {
    let (_temp, path) = fixture(
        "from ast import Constant\na = Constant(\"x\", kind=None, lineno=1, col_offset=2, end_lineno=None, end_col_offset=3)\nb = Constant(value=Ellipsis)\n",
    );
    for target in [
        PythonVersion::Py312,
        PythonVersion::Py313,
        PythonVersion::Py314,
    ] {
        assert!(check(std::slice::from_ref(&path), target).is_empty());
    }

    for bad in [
        "from ast import Constant\nbad = Constant(\"x\", lineno=\"wrong\")\n",
        "from ast import Constant\nbad = Constant(\"x\", unrelated=1)\n",
        "from ast import Constant\nbad = Constant()\n",
    ] {
        let (_temp, path) = fixture(bad);
        for target in [
            PythonVersion::Py312,
            PythonVersion::Py313,
            PythonVersion::Py314,
        ] {
            assert!(!check(std::slice::from_ref(&path), target).is_empty());
        }
    }
}

#[test]
fn any_control_and_local_ast_shadow_do_not_use_the_native_special_case() {
    let (_temp, path) = fixture(
        "from typing import Any\ndef render(value: Any) -> str:\n    return f\"{value}\"\n",
    );
    for target in [
        PythonVersion::Py312,
        PythonVersion::Py313,
        PythonVersion::Py314,
    ] {
        assert!(check(std::slice::from_ref(&path), target).is_empty());
    }

    let temp = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(temp.path()).unwrap();
    let local_ast = root.join("ast.py");
    let main = root.join("main.py");
    fs::write(&local_ast, "class Constant:\n    value: str\n").unwrap();
    fs::write(
        &main,
        "import ast\ndef render(value: ast.Constant) -> str:\n    return f\"{value.value}\"\n",
    )
    .unwrap();
    for target in [
        PythonVersion::Py312,
        PythonVersion::Py313,
        PythonVersion::Py314,
    ] {
        let diagnostics = check(&[main.clone(), local_ast.clone()], target);
        assert!(
            diagnostics.is_empty(),
            "local ast shadow target {}: {diagnostics:?}",
            target.name()
        );
    }
}

#[test]
fn legacy_aliases_follow_target_and_current_deprecated_code_setting() {
    let (_temp, path) = fixture(
        "from ast import Constant\nvalue = Constant(\"x\")\nvalue.n\nvalue.s\nvalue.n = \"y\"\nvalue.s = b\"y\"\n",
    );
    for target in [PythonVersion::Py312, PythonVersion::Py313] {
        // The pinned strict profile leaves mypy's `deprecated` code disabled.
        assert_eq!(
            check(std::slice::from_ref(&path), target),
            Vec::<(String, String)>::new()
        );
    }

    let (_temp, path) =
        fixture("from ast import Constant\nvalue = Constant(\"x\")\nvalue.n\nvalue.s\n");
    assert_eq!(
        check(&[path], PythonVersion::Py314),
        vec![
            (
                "attr-defined".into(),
                "\"Constant\" has no attribute \"n\"".into()
            ),
            (
                "attr-defined".into(),
                "\"Constant\" has no attribute \"s\"".into()
            ),
        ],
    );

    let (_temp, path) =
        fixture("from ast import Constant\nvalue = Constant(\"x\")\nvalue.n = []\n");
    assert!(!check(&[path], PythonVersion::Py312).is_empty());
}

#[test]
fn property_class_access_follows_target_and_replace_preserves_subclass() {
    let (_temp, path) = fixture(
        "from ast import Constant\nclass Child(Constant):\n    pass\nvalue: Child = Child(\"x\").__replace__(value=b\"x\", lineno=1)\n",
    );
    assert!(!check(std::slice::from_ref(&path), PythonVersion::Py312).is_empty());
    assert!(check(&[path], PythonVersion::Py314).is_empty());

    let (_temp, path) = fixture("from ast import Constant\nvalue = Constant.n\n");
    for target in [PythonVersion::Py312, PythonVersion::Py313] {
        // Property class loads are accepted by the current checker path.
        assert_eq!(
            check(std::slice::from_ref(&path), target),
            Vec::<(String, String)>::new()
        );
    }
    assert_eq!(
        check(&[path], PythonVersion::Py314),
        vec![(
            "attr-defined".into(),
            "\"type[Constant]\" has no attribute \"n\"".into(),
        )],
    );
}
