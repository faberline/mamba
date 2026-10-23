//! Local source ownership controls confirmed against pinned mypy 2.4.0 originals.
use crate::domain::{ports::ProjectStore, source::PythonVersion};
use crate::infrastructure::filesystem::Filesystem;
use std::{
    fs,
    path::{Path, PathBuf},
};

const TARGETS: [PythonVersion; 3] = [
    PythonVersion::Py312,
    PythonVersion::Py313,
    PythonVersion::Py314,
];

fn fixture(inside_repository: bool) -> tempfile::TempDir {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let base = if inside_repository {
        repo.join("target")
    } else {
        PathBuf::from("/tmp")
    };
    let temp = tempfile::Builder::new()
        .prefix("mamba-owner-regression-")
        .tempdir_in(&base)
        .unwrap();
    let actual = fs::canonicalize(temp.path()).unwrap();
    assert_eq!(
        actual.starts_with(fs::canonicalize(repo).unwrap()),
        inside_repository
    );
    temp
}

fn write(root: &Path, relative: &str, text: &str) -> PathBuf {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, text).unwrap();
    path
}

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

fn bytes_warning() -> Vec<(String, String)> {
    vec![("str-bytes-safe".into(), "If x = b'abc' then f\"{x}\" or \"{}\".format(x) produces \"b'abc'\", not \"abc\". If this is desired behavior, use f\"{x!r}\" or \"{!r}\".format(x). Otherwise, decode the bytes".into())]
}

#[test]
fn explicit_local_ast_owner_survives_both_import_forms_and_temp_locations() {
    for inside in [true, false] {
        for source in [
            "import ast\ndef render(value: ast.Constant) -> str:\n    return f\"{value.value}\"\n",
            "from ast import Constant\ndef render(value: Constant) -> str:\n    return f\"{value.value}\"\n",
        ] {
            let temp = fixture(inside);
            let root = fs::canonicalize(temp.path()).unwrap();
            let local = write(&root, "ast.py", "class Constant:\n    value: str\n");
            let main = write(&root, "main.py", source);
            for target in TARGETS {
                let actual = check(&[main.clone(), local.clone()], target);
                assert!(
                    actual.is_empty(),
                    "inside={inside} target={}: {actual:?}",
                    target.name()
                );
            }
        }
    }
}

#[test]
fn local_package_relative_import_field_types_remain_strict() {
    let temp = fixture(true);
    let root = fs::canonicalize(temp.path()).unwrap();
    write(&root, "pkg/__init__.py", "");
    write(&root, "pkg/fields.py", "class Payload:\n    value: str\n");
    let model = write(
        &root,
        "pkg/models.py",
        "from .fields import Payload\nclass Constant:\n    value: Payload\n",
    );
    let main = write(
        &root,
        "main.py",
        "from pkg.models import Constant\ndef render(value: Constant) -> str:\n    return value.value.value\n",
    );
    for target in TARGETS {
        let actual = check(&[main.clone(), model.clone()], target);
        assert!(actual.is_empty(), "target={}: {actual:?}", target.name());
    }
}

#[test]
fn another_modules_constant_does_not_replace_stdlib_constant() {
    let temp = fixture(true);
    let root = fs::canonicalize(temp.path()).unwrap();
    let other = write(&root, "other.py", "class Constant:\n    value: str\n");
    let main = write(
        &root,
        "main.py",
        "import ast\nfrom other import Constant\ndef local(value: Constant) -> str:\n    return f\"{value.value}\"\ndef standard(value: ast.Constant) -> str:\n    return f\"{value.value}\"\n",
    );
    for target in TARGETS {
        assert_eq!(
            check(&[main.clone(), other.clone()], target),
            bytes_warning(),
            "target={}",
            target.name()
        );
    }
}

#[test]
fn ast_file_outside_search_roots_does_not_disable_stdlib_model() {
    let temp = fixture(true);
    let root = fs::canonicalize(temp.path()).unwrap();
    write(
        &root,
        "unselected/ast.py",
        "class Constant:\n    value: str\n",
    );
    let main = write(
        &root,
        "main.py",
        "import ast\ndef render(value: ast.Constant) -> str:\n    return f\"{value.value}\"\n",
    );
    for target in TARGETS {
        assert_eq!(
            check(std::slice::from_ref(&main), target),
            bytes_warning(),
            "target={}",
            target.name()
        );
    }
}
