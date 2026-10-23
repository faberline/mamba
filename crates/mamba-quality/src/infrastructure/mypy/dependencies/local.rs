//! Local imports follow source search roots and prefer package type stubs.
mod catalog;
#[cfg(test)]
mod tests;
use crate::domain::source::QualityWorkspace;
pub(super) use catalog::Catalog;
use std::{collections::BTreeSet, path::PathBuf};

#[derive(Clone, Copy)]
enum ResolvedPath {
    Canonical,
    Lexical,
}

pub(super) fn roots(workspace: &QualityWorkspace) -> Vec<Vec<PathBuf>> {
    workspace
        .projects
        .iter()
        .enumerate()
        .map(|(project, info)| {
            let mut roots = BTreeSet::from([info.root.clone()]);
            for source in workspace
                .sources
                .iter()
                .filter(|source| source.project == project)
            {
                if let Some(parent) = source.path.parent() {
                    let mut root = parent;
                    while root.join("__init__.py").is_file() || root.join("__init__.pyi").is_file()
                    {
                        let Some(parent) = root.parent() else {
                            break;
                        };
                        root = parent;
                    }
                    roots.insert(root.to_path_buf());
                }
            }
            roots.into_iter().collect()
        })
        .collect()
}

#[cfg(test)]
pub(super) fn sources(
    roots: &[PathBuf],
    name: &str,
    errors: &mut BTreeSet<String>,
) -> Vec<(PathBuf, String)> {
    sources_with_catalog_mode(
        roots,
        name,
        &mut Catalog::uncached(),
        errors,
        ResolvedPath::Canonical,
    )
}

pub(super) fn sources_for_diagnostic(
    roots: &[PathBuf],
    name: &str,
    catalog: &mut Catalog,
    errors: &mut BTreeSet<String>,
) -> Vec<(PathBuf, String)> {
    sources_with_catalog_mode(roots, name, catalog, errors, ResolvedPath::Lexical)
}

pub(super) fn sources_with_catalog(
    roots: &[PathBuf],
    name: &str,
    catalog: &mut Catalog,
    errors: &mut BTreeSet<String>,
) -> Vec<(PathBuf, String)> {
    sources_with_catalog_mode(roots, name, catalog, errors, ResolvedPath::Canonical)
}

fn sources_with_catalog_mode(
    roots: &[PathBuf],
    name: &str,
    catalog: &mut Catalog,
    errors: &mut BTreeSet<String>,
    resolved_path: ResolvedPath,
) -> Vec<(PathBuf, String)> {
    let parts: Vec<_> = name.split('.').filter(|part| !part.is_empty()).collect();
    for root in roots {
        let mut current = root.clone();
        let mut module = String::new();
        let mut selected = Vec::new();
        for (offset, part) in parts.iter().enumerate() {
            current.push(part);
            if offset > 0 {
                module.push('.');
            }
            module.push_str(part);
            let directory_possible = catalog.may_exist(&current);
            let candidates = [
                current.join("__init__.pyi"),
                current.join("__init__.py"),
                current.with_extension("pyi"),
                current.with_extension("py"),
            ];
            let selected_path = candidates.iter().enumerate().find_map(|(index, path)| {
                if index < 2 && !directory_possible {
                    catalog.skip_probe();
                    return None;
                }
                (catalog.is_file(path, errors)
                    && (index >= 2 || catalog.has_exact_name(&current, errors)))
                .then_some(path)
            });
            if let Some(path) = selected_path {
                match path.canonicalize() {
                    Ok(canonical_path) => {
                        let resolved_path = match resolved_path {
                            ResolvedPath::Canonical => canonical_path,
                            ResolvedPath::Lexical => path.clone(),
                        };
                        selected.push((resolved_path, module.clone()));
                    }
                    Err(error) => {
                        errors.insert(format!("{}: {error}", path.display()));
                    }
                }
                if path
                    .file_name()
                    .is_some_and(|name| name != "__init__.py" && name != "__init__.pyi")
                {
                    break;
                }
            } else if !catalog.is_dir(&current, errors) {
                break;
            }
        }
        if !selected.is_empty() {
            return selected;
        }
    }
    Vec::new()
}
