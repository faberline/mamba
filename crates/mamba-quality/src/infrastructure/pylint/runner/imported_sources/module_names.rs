//! Dependency module names retain package and namespace import identities.
use crate::{
    domain::source::QualityWorkspace,
    infrastructure::project::{CachedDocument, ProjectIndex},
};
use std::collections::BTreeMap;

pub(super) fn scanned(workspace: &QualityWorkspace, names: &mut BTreeMap<usize, String>) {
    for (id, source) in workspace.sources.iter().enumerate() {
        let root = &workspace.projects[source.project].root;
        let highest = source
            .path
            .parent()
            .into_iter()
            .flat_map(|parent| parent.ancestors())
            .take_while(|parent| parent.starts_with(root) && parent.join("__init__.py").is_file())
            .last();
        if let Some(base) = highest.and_then(|parent| parent.parent()) {
            if let Ok(relative) = source.path.strip_prefix(base) {
                let mut parts: Vec<_> = relative
                    .iter()
                    .map(|part| part.to_string_lossy().into_owned())
                    .collect();
                let stem = source.path.file_stem().unwrap().to_string_lossy();
                parts.pop();
                if stem != "__init__" {
                    parts.push(stem.into_owned());
                }
                names.insert(id, parts.join("."));
                continue;
            }
        }
        if workspace.explicit_files.contains(&source.path) {
            continue;
        }
        for root in &workspace.scan_roots {
            let Ok(relative) = source.path.strip_prefix(root) else {
                continue;
            };
            let Some(parent) = source.path.parent() else {
                continue;
            };
            if parent != root && !parent.join("__init__.py").is_file() {
                let mut parts: Vec<_> = relative
                    .iter()
                    .map(|p| p.to_string_lossy().into_owned())
                    .collect();
                if let Some(last) = parts.last_mut() {
                    *last = source
                        .path
                        .file_stem()
                        .unwrap()
                        .to_string_lossy()
                        .into_owned();
                }
                names.insert(id, parts.join("."));
            }
            break;
        }
    }
}

pub(super) fn index<'a>(
    workspace: &'a QualityWorkspace,
    names: &BTreeMap<usize, String>,
    documents: &[CachedDocument],
) -> ProjectIndex<'a> {
    let mut index = ProjectIndex::with_documents(workspace, documents);
    for (id, module) in index.modules.iter_mut().enumerate() {
        if let Some(name) = names.get(&id) {
            module.name.clone_from(name);
        }
    }
    index.names.clear();
    index.namespaces.clear();
    for (id, module) in index.modules.iter().enumerate() {
        if workspace.sources[id].is_stub && !module.is_package {
            continue;
        }
        let key = (module.project, module.name.clone());
        if !index.names.contains_key(&key) || !module.is_package {
            index.names.insert(key, id);
        }
    }
    for (id, module) in index.modules.iter().enumerate() {
        if workspace.sources[id].is_stub && !module.is_package {
            continue;
        }
        index
            .namespaces
            .insert((module.project, module.name.clone()));
        for (offset, _) in module.name.match_indices('.') {
            index
                .namespaces
                .insert((module.project, module.name[..offset].into()));
        }
    }
    index
}
