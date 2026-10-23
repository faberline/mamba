//! Read-only type data. These Python stubs are data, never executed code.
use crate::domain::source::DependencySource;
use include_dir::{include_dir, Dir, DirEntry};
use std::{path::Path, sync::OnceLock};

mod module_index;

static STDLIB: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/assets/typeshed/stdlib");
static PROVENANCE: &str = include_str!("../../assets/typeshed/provenance.json");

pub(crate) fn module(name: &str) -> Option<&'static str> {
    module_index::module(name)
}

/// The original asset path carries package identity through `__init__.pyi`.
pub(crate) struct ModuleSource {
    pub text: &'static str,
    pub path: &'static Path,
}

pub(crate) fn module_source(name: &str) -> Option<ModuleSource> {
    module_index::module_source(name)
}

fn count(directory: &Dir<'_>) -> usize {
    directory
        .entries()
        .iter()
        .map(|entry| match entry {
            DirEntry::Dir(directory) => count(directory),
            DirEntry::File(_) => 1,
        })
        .sum()
}

pub fn source() -> DependencySource {
    static SOURCE: OnceLock<DependencySource> = OnceLock::new();
    SOURCE
        .get_or_init(|| {
            let provenance: serde_json::Value =
                serde_json::from_str(PROVENANCE).expect("embedded type-data provenance");
            DependencySource {
                kind: "embedded-type-stubs".into(),
                path: None,
                identity: format!(
                    "mypy 2.4.0 typeshed standard library ({} data files)",
                    count(&STDLIB)
                ),
                sha256: Some(
                    provenance["tree_sha256"]
                        .as_str()
                        .expect("embedded type-data hash")
                        .into(),
                ),
            }
        })
        .clone()
}
