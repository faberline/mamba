//! Whole embedded modules enter the same inference cache as typed dependencies.
use crate::{
    domain::source::{SourceEncoding, SourceSnapshot},
    infrastructure::{embedded, metadata},
};
use mamba_language::application::quality_syntax::PythonTarget;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::OnceLock,
};

// Compiler-owned forms, structural types and native binders keep their bootstrap
// providers. Ordinary modules are discovered only through actual imports.
const BOOTSTRAP: &[&str] = &[
    "builtins",
    "typing",
    "typing_extensions",
    "types",
    "sys",
    "abc",
    "collections.abc",
    "_collections_abc",
    "dataclasses",
    "functools",
    "string.templatelib",
];
const ROOT: &str = "/__mamba_embedded_typeshed__";

// Source declarations supply real class binders. Missing exports retain the
// existing bootstrap providers, including their known dynamic module metadata.
pub(in crate::infrastructure::mypy) fn preserves_bootstrap_members(name: &str) -> bool {
    name == "collections"
}

pub(super) fn path(name: &str, target: PythonTarget) -> Option<PathBuf> {
    if BOOTSTRAP.contains(&name) || !target_available(name, target) {
        return None;
    }
    let module = embedded::module_source(name)?;
    Some(Path::new(ROOT).join(module.path))
}

pub(super) fn source(project: usize, name: &str, target: PythonTarget) -> Option<SourceSnapshot> {
    let path = path(name, target)?;
    let module = embedded::module_source(name)?;
    let bytes = module.text.as_bytes().to_vec();
    Some(SourceSnapshot {
        path,
        project,
        sha256: metadata::sha256(&bytes),
        bytes,
        text: module.text.into(),
        encoding: SourceEncoding::Utf8 { bom: false },
        decoding_issue: None,
        is_stub: true,
    })
}

pub(in crate::infrastructure::mypy) fn available(name: &str, target: PythonTarget) -> bool {
    embedded::module_source(name).is_some() && target_available(name, target)
}

pub(in crate::infrastructure::mypy) fn is_source(source: &SourceSnapshot) -> bool {
    source.is_stub && source.path.starts_with(ROOT)
}

pub(in crate::infrastructure::mypy) fn selects_declarations(
    name: &str,
    source: &SourceSnapshot,
) -> bool {
    source.is_stub
        && !BOOTSTRAP.contains(&name)
        && embedded::module_source(name)
            .is_some_and(|module| source.path == Path::new(ROOT).join(module.path))
}

type Version = (u8, u8);
type Versions = BTreeMap<&'static str, (Version, Option<Version>)>;

fn target_available(name: &str, target: PythonTarget) -> bool {
    static VERSIONS: OnceLock<Versions> = OnceLock::new();
    let versions = VERSIONS.get_or_init(|| {
        include_str!("../../../../assets/typeshed/stdlib/VERSIONS")
            .lines()
            .filter_map(|line| {
                let line = line.split('#').next().expect("version line").trim();
                if line.is_empty() {
                    return None;
                }
                let (module, range) = line.split_once(':').expect("locked module lifetime");
                let (minimum, maximum) = range.split_once('-').expect("locked version range");
                Some((
                    module.trim(),
                    (
                        parse_version(minimum),
                        (!maximum.trim().is_empty()).then(|| parse_version(maximum)),
                    ),
                ))
            })
            .collect()
    });
    let Some(&(minimum, maximum)) = versions
        .get(name)
        .or_else(|| versions.get(name.split('.').next().expect("module name")))
    else {
        return true;
    };
    let version = match target {
        PythonTarget::Py312 => (3, 12),
        PythonTarget::Py313 => (3, 13),
        PythonTarget::Py314 => (3, 14),
    };
    version >= minimum && maximum.is_none_or(|maximum| version <= maximum)
}

fn parse_version(version: &str) -> Version {
    let (major, minor) = version
        .trim()
        .split_once('.')
        .expect("locked Python version");
    (
        major.parse().expect("locked major version"),
        minor.parse().expect("locked minor version"),
    )
}
