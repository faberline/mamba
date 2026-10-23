//! Pinned SourceFinder.keyfunc and directory-local .pyi/directory precedence.
use crate::domain::source::{RequestedSource, SourceRequest};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

pub(super) fn sources(request: &SourceRequest) -> Vec<&RequestedSource> {
    if !request.directory {
        return request.sources.iter().collect();
    }
    let candidates: Vec<_> = request.sources.iter().collect();
    directory(&request.path, &candidates)
}

fn sort_key(name: &str) -> (bool, i8, String) {
    let path = Path::new(name);
    let stem = path.file_stem().unwrap_or_default().to_string_lossy();
    let priority = match path.extension().and_then(|s| s.to_str()) {
        Some("pyi") => 0,
        Some("py") => 1,
        _ => -1,
    };
    (
        stem != "__init__",
        priority,
        if priority < 0 {
            name.into()
        } else {
            stem.into_owned()
        },
    )
}

fn directory<'a>(path: &Path, candidates: &[&'a RequestedSource]) -> Vec<&'a RequestedSource> {
    let mut children: BTreeMap<String, (bool, Vec<&RequestedSource>)> = BTreeMap::new();
    for &source in candidates {
        let Ok(relative) = source.path.strip_prefix(path) else {
            continue;
        };
        let mut parts = relative.iter();
        let Some(first) = parts.next() else { continue };
        let is_directory = parts.next().is_some();
        let entry = children
            .entry(first.to_string_lossy().into_owned())
            .or_default();
        entry.0 = is_directory;
        entry.1.push(source);
    }
    let mut children: Vec<_> = children.into_iter().collect();
    children.sort_by_key(|(name, _)| sort_key(name));
    let mut seen = BTreeSet::new();
    let mut sources = Vec::new();
    for (name, (is_directory, children)) in children {
        if name.starts_with('.')
            || matches!(
                name.as_str(),
                "__pycache__" | "site-packages" | "node_modules"
            )
        {
            continue;
        }
        if is_directory {
            let nested = directory(&path.join(&name), &children);
            if !nested.is_empty() {
                seen.insert(name);
                sources.extend(nested);
            }
        } else {
            let stem = Path::new(&name)
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            if seen.insert(stem) {
                sources.extend(children);
            }
        }
    }
    sources
}
