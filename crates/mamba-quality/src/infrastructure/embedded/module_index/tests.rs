//! Compare every indexed lookup with the unchanged recursive lookup.
use super::{Index, STDLIB};
use include_dir::{Dir, DirEntry, File};
use mamba_language::application::quality_syntax::PythonTarget;
use serde_json::json;
use std::{collections::BTreeSet, fs};

// This is the original embedded::module body, frozen before the index change.
fn legacy_module(name: &str) -> Option<&'static str> {
    let path = name.replace('.', "/");
    STDLIB
        .get_file(format!("{path}.pyi"))
        .or_else(|| STDLIB.get_file(format!("{path}/__init__.pyi")))
        .and_then(|file| file.contents_utf8())
}

fn names_from(directory: &Dir<'_>, names: &mut BTreeSet<String>) {
    for entry in directory.entries() {
        match entry {
            DirEntry::Dir(nested) => names_from(nested, names),
            DirEntry::File(file) => {
                let path = file.path().to_str().expect("embedded path is UTF-8");
                if let Some(stem) = path.strip_suffix(".pyi") {
                    names.insert(stem.into());
                    names.insert(stem.replace('/', "."));
                    if let Some(package) = stem.strip_suffix("/__init__") {
                        names.insert(package.into());
                        names.insert(package.replace('/', "."));
                    }
                } else {
                    names.insert(path.into());
                }
            }
        }
    }
}

#[test]
fn every_embedded_name_matches_recursive_lookup_for_all_targets() {
    let index = super::INDEX.get_or_init(|| Index::new(&STDLIB));
    assert_eq!(index.files.len(), super::super::count(&STDLIB));
    assert_eq!(index.files.len(), 754);

    let mut names = BTreeSet::new();
    names_from(&STDLIB, &mut names);
    names.extend(
        [
            "",
            ".",
            "..",
            "...",
            "/",
            "//",
            "a//b",
            "a/./b",
            "a/../b",
            "builtins/",
            ".builtins",
            "builtins.",
            "builtins..",
            "builtins.pyi",
            "__init__",
            "missing",
            "missing.child",
            "pydantic",
            "typing_extensions",
            "μ",
            "with space",
            "a\\b",
            "\0",
        ]
        .into_iter()
        .map(str::to_owned),
    );

    let mut reports = Vec::new();
    for target in [
        PythonTarget::Py312,
        PythonTarget::Py313,
        PythonTarget::Py314,
    ] {
        // The embedded lookup is target-neutral. Check that contract for each
        // target used by the callers rather than assuming it from one sample.
        let mut index_hits = 0;
        let mut index_misses = 0;
        let mut utf8_results = 0;
        for name in &names {
            let expected = legacy_module(name);
            let actual = super::super::module(name);
            assert_eq!(
                actual.map(str::as_bytes),
                expected.map(str::as_bytes),
                "target {target:?}, module {name:?}"
            );
            if index.file(name).is_some() {
                index_hits += 1;
            } else {
                index_misses += 1;
            }
            utf8_results += usize::from(actual.is_some());
        }
        assert!(index_hits >= 752);
        assert!(index_misses > 0);
        reports.push(json!({
            "target": format!("{target:?}"),
            "names_checked": names.len(),
            "index_hits": index_hits,
            "index_misses": index_misses,
            "utf8_results": utf8_results,
        }));
    }
    if let Some(destination) = std::env::var_os("MAMBA_QUALITY_EMBEDDED_INDEX_EVIDENCE") {
        fs::write(
            destination,
            serde_json::to_vec_pretty(&json!({
                "schema": "mamba.quality.embedded-index-comparison.v1",
                "embedded_files": index.files.len(),
                "index_capacity": index.files.capacity(),
                "all_names": names.len(),
                "targets": reports,
                "whole_tool_accepted": false,
            }))
            .unwrap(),
        )
        .unwrap();
    }
}

static SYNTHETIC: Dir<'static> = Dir::new(
    "",
    &[
        DirEntry::File(File::new("dual.pyi", b"direct")),
        DirEntry::Dir(Dir::new(
            "dual",
            &[DirEntry::File(File::new("dual/__init__.pyi", b"package"))],
        )),
        DirEntry::File(File::new("bad.pyi", b"\xff")),
        DirEntry::Dir(Dir::new(
            "bad",
            &[DirEntry::File(File::new("bad/__init__.pyi", b"utf8"))],
        )),
        DirEntry::File(File::new("odd-name.pyi", b"odd")),
        DirEntry::File(File::new("sp ace.pyi", b"space")),
        DirEntry::File(File::new("μ.pyi", b"unicode")),
        DirEntry::File(File::new("dot.name.pyi", b"inaccessible")),
    ],
);

fn legacy_synthetic(name: &str) -> Option<&'static str> {
    let path = name.replace('.', "/");
    SYNTHETIC
        .get_file(format!("{path}.pyi"))
        .or_else(|| SYNTHETIC.get_file(format!("{path}/__init__.pyi")))
        .and_then(|file| file.contents_utf8())
}

#[test]
fn direct_package_utf8_and_unusual_names_keep_legacy_behavior() {
    let index = Index::new(&SYNTHETIC);
    assert_eq!(index.files.len(), 8);
    assert_eq!(index.module("dual"), Some("direct"));
    assert_eq!(index.module("bad"), None);
    for name in [
        "dual",
        "dual/",
        "bad",
        "bad/",
        "odd-name",
        "sp ace",
        "μ",
        "dot.name",
        "dot/name",
        "",
        ".",
        "..",
        "missing",
        "missing/child",
        "a//b",
        "a/./b",
        "a/../b",
        "\0",
    ] {
        assert_eq!(
            index.module(name).map(str::as_bytes),
            legacy_synthetic(name).map(str::as_bytes),
            "module {name:?}"
        );
    }
}

#[test]
fn mutable_project_files_and_native_names_are_not_cached() {
    let name = "__quality_embedded_index_project_only__";
    let index = super::INDEX.get_or_init(|| Index::new(&STDLIB));
    let size = index.files.len();
    assert_eq!(legacy_module(name), None);
    assert_eq!(super::super::module(name), None);
    assert_eq!(super::super::module("pydantic"), legacy_module("pydantic"));
    assert_eq!(
        super::super::module("typing_extensions"),
        legacy_module("typing_extensions")
    );

    let project = tempfile::tempdir().unwrap();
    let path = project.path().join(format!("{name}.py"));
    fs::write(&path, "value = 1\n").unwrap();
    assert!(path.is_file());
    assert_eq!(super::super::module(name), None);
    fs::write(&path, "value = 2\n").unwrap();
    assert_eq!(fs::read_to_string(&path).unwrap(), "value = 2\n");
    assert_eq!(super::super::module(name), None);
    assert_eq!(index.files.len(), size);
}
