use super::*;
use crate::{domain::ports::ProjectStore, infrastructure::filesystem::Filesystem};
use std::fs;
mod ast_errors;
mod catalog;
mod excluded_sources;
mod literal_errors;
mod literal_interactions;
mod literal_syntax;
mod literal_values;
mod public_cr_stream;
mod reference;
mod unicode_data;

#[test]
fn uses_each_python_versions_contextual_lowercase_mapping() {
    let cases: serde_json::Value = serde_json::from_str(unicode_data::CASES).unwrap();
    for (target, version) in [
        crate::domain::source::PythonVersion::Py312,
        crate::domain::source::PythonVersion::Py313,
        crate::domain::source::PythonVersion::Py314,
    ]
    .into_iter()
    .enumerate()
    {
        for case in cases.as_array().unwrap() {
            let source = case["source"].as_str().unwrap();
            assert_eq!(
                sorting::characters(version).lowercase(source),
                case["expected"][target].as_str().unwrap(),
                "{} {source:?}",
                version.name(),
            );
        }
    }
}

#[test]
fn groups_sections_splits_imports_and_retains_aliases() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("main.py");
    fs::write(
        &path,
        "import z\nimport sys, os  # trailing\nfrom os import path as p, getenv\n\nvalue=1\n",
    )
    .unwrap();
    let workspace = Filesystem.load(std::slice::from_ref(&path), true);
    let (output, remaining) =
        format(&workspace.sources[0], &workspace, &FixedProfile::locked()).unwrap();
    assert!(remaining.is_empty());
    assert_eq!(
        output,
        "import os\nimport sys  # trailing\nfrom os import getenv\nfrom os import path as p\n\nimport z\n\nvalue=1\n"
    );
}

#[test]
fn honours_off_blocks_and_relative_depth() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("main.py");
    fs::write(&path, "# isort: off\nimport z\nimport os\n# isort: on\nfrom . import x\nfrom ...a import z\nfrom ..b import y\n").unwrap();
    let workspace = Filesystem.load(std::slice::from_ref(&path), true);
    let (output, _) = format(&workspace.sources[0], &workspace, &FixedProfile::locked()).unwrap();
    assert_eq!(
        output,
        "# isort: off\nimport z\nimport os\n\n# isort: on\nfrom ...a import z\nfrom ..b import y\nfrom . import x\n"
    );
}

#[test]
fn matches_reference_alias_order_and_ignores_an_off_marker_inside_a_string() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("main.py");
    fs::write(
        &path,
        "NOTE = \"# isort: off\"\nfrom package import Zed as Z, apple, Beta as B\n",
    )
    .unwrap();
    let workspace = Filesystem.load(std::slice::from_ref(&path), true);
    let (output, _) = format(&workspace.sources[0], &workspace, &FixedProfile::locked()).unwrap();
    assert_eq!(
        output,
        "NOTE = \"# isort: off\"\nfrom package import Beta as B\nfrom package import Zed as Z\nfrom package import apple\n"
    );
}

mod split_boundaries;

mod insertion_markers;

mod fixed_points;
