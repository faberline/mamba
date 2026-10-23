//! Packages constrain child searches; namespace portions retain root order.
use crate::domain::source::DependencySource;
use std::{
    collections::BTreeSet,
    fs, io,
    path::{Path, PathBuf},
};

fn metadata(path: &Path, errors: &mut BTreeSet<String>) -> Option<fs::Metadata> {
    match fs::metadata(path) {
        Ok(metadata) => Some(metadata),
        Err(error)
            if matches!(
                error.kind(),
                io::ErrorKind::NotFound | io::ErrorKind::NotADirectory
            ) =>
        {
            None
        }
        Err(error) => {
            errors.insert(format!("{}: {error}", path.display()));
            None
        }
    }
}

pub(super) fn sources(
    dependencies: &[DependencySource],
    name: &str,
    errors: &mut BTreeSet<String>,
) -> Vec<(PathBuf, String)> {
    let mut roots: Vec<_> = dependencies
        .iter()
        .filter_map(|dependency| dependency.path.as_ref())
        .filter(|path| metadata(path, errors).is_some_and(|metadata| metadata.is_dir()))
        .cloned()
        .collect();
    let mut sources = Vec::new();
    let mut module = String::new();
    for part in name.split('.') {
        if !module.is_empty() {
            module.push('.');
        }
        module.push_str(part);
        let mut namespaces = Vec::new();
        let mut selected = None;
        for root in &roots {
            let directory = root.join(part);
            let package = ["__init__.py", "__init__.pyi"]
                .into_iter()
                .map(|name| directory.join(name))
                .find(|path| metadata(path, errors).is_some_and(|metadata| metadata.is_file()));
            if let Some(package) = package {
                selected = Some((package, Some(directory)));
                break;
            }
            let file = root.join(format!("{part}.py"));
            if metadata(&file, errors).is_some_and(|metadata| metadata.is_file()) {
                selected = Some((file, None));
                break;
            }
            if metadata(&directory, errors).is_some_and(|metadata| metadata.is_dir()) {
                namespaces.push(directory);
            }
        }
        if let Some((path, directory)) = selected {
            match path.canonicalize() {
                Ok(path) => sources.push((path, module.clone())),
                Err(error) => {
                    errors.insert(format!("{}: {error}", path.display()));
                }
            }
            let Some(directory) = directory else {
                break;
            };
            roots = vec![directory];
        } else if namespaces.is_empty() {
            break;
        } else {
            roots = namespaces;
        }
    }
    sources
}
