//! Fixed source-root and namespace rules adapted from isort 9.0.2.
use crate::domain::source::ProjectInfo;
use std::{
    fs,
    path::{Path, PathBuf},
};

fn exists(path: &Path) -> bool {
    let Some(parent) = path.parent() else {
        return false;
    };
    let Some(name) = path.file_name() else {
        return false;
    };
    fs::read_dir(parent).is_ok_and(|mut entries| {
        entries.any(|entry| entry.is_ok_and(|entry| entry.file_name() == name))
    })
}

fn package(path: &Path) -> bool {
    exists(path) && path.is_dir()
}

fn module(path: &Path, minor: u8) -> bool {
    if exists(&path.with_extension("py"))
        || exists(&path.join("__init__.py"))
        || exists(&path.with_extension("so"))
        || exists(&path.with_extension("abi3.so"))
    {
        return true;
    }
    let Some(parent) = path.parent() else {
        return false;
    };
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    let prefix = format!("{name}.cpython-3{minor}-");
    fs::read_dir(parent).is_ok_and(|mut entries| {
        entries.any(|entry| {
            entry.is_ok_and(|entry| {
                let name = entry.file_name();
                let name = name.to_string_lossy();
                name.starts_with(&prefix) && name.ends_with(".so")
            })
        })
    })
}

fn namespace(path: &Path) -> bool {
    if !package(path) {
        return false;
    }
    let init = path.join("__init__.py");
    if init.is_file() {
        let Ok(bytes) = fs::read(init) else {
            return false;
        };
        let prefix = String::from_utf8_lossy(&bytes[..bytes.len().min(4096)]);
        return [
            "__import__('pkg_resources').declare_namespace(__name__)",
            "__import__(\"pkg_resources\").declare_namespace(__name__)",
            "__path__ = __import__('pkgutil').extend_path(__path__, __name__)",
            "__path__ = __import__(\"pkgutil\").extend_path(__path__, __name__)",
        ]
        .iter()
        .any(|marker| prefix.contains(marker));
    }
    let Ok(entries) = fs::read_dir(path) else {
        return false;
    };
    !entries.flatten().any(|entry| {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
        name == "setup.cfg"
            || name == "pyproject.toml"
            || path.extension().is_some_and(|extension| {
                ["py", "pyi", "pyx", "pxd"]
                    .iter()
                    .any(|suffix| extension == *suffix)
            })
    })
}

fn source_path(name: &str, roots: &[PathBuf], minor: u8, nested: bool) -> bool {
    let (base, rest) = name
        .split_once('.')
        .map_or((name, None), |(base, rest)| (base, Some(rest)));
    for root in roots {
        let mut path = root.join(base);
        if !nested && !path.is_dir() && root.file_name().is_some_and(|name| name == base) {
            path = root.clone();
        }
        if let Some(rest) = rest {
            if namespace(&path) {
                return source_path(rest, &[path], minor, true);
            }
        }
        if module(&path, minor)
            || package(&path)
            || (root.file_name().is_some_and(|name| name == base) && package(root))
        {
            return true;
        }
    }
    false
}

pub fn first_party(name: &str, project: &ProjectInfo) -> bool {
    let roots: Vec<_> = project
        .dependencies
        .iter()
        .filter(|source| source.kind == "project")
        .filter_map(|source| source.path.clone())
        .collect();
    source_path(name, &roots, project.python_version.minor(), false)
}
