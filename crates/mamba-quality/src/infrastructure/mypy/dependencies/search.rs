//! Installed Python packages require a typed marker or a stub distribution.
use super::local::Catalog;
use crate::domain::source::DependencySource;
use mamba_language::application::quality_syntax::PythonTarget;
use std::{collections::BTreeSet, fs, path::PathBuf};

// This request-local query proves only the narrow case where no filesystem
// root has any top-level shape for the child. It does not select or load a
// source. A present parent, namespace, provider or uncertain root stays unknown.
pub(super) struct ChildAbsence {
    standard: serde_json::Value,
    children: serde_json::Value,
}

impl ChildAbsence {
    pub(super) fn new() -> Option<Self> {
        Some(Self {
            standard: serde_json::from_str(include_str!(
                "../../../../assets/stdlib/standard-module-symbols.json"
            ))
            .ok()?,
            children: serde_json::from_str(include_str!(
                "../../../../assets/stdlib/typed-child-modules.json"
            ))
            .ok()?,
        })
    }

    pub(super) fn absent(
        &self,
        name: &str,
        target: PythonTarget,
        roots: &[PathBuf],
        dependencies: &[DependencySource],
        catalog: &mut Catalog,
        errors: &mut BTreeSet<String>,
    ) -> bool {
        // A complete listing cannot prove absence beyond opaque search paths.
        if dependencies
            .iter()
            .any(|dependency| dependency.kind == "unsupported-package-path")
        {
            return false;
        }
        if !name.contains('.')
            || name
                .split('.')
                .any(|part| part.is_empty() || part.contains('/') || part.contains('\\'))
            || crate::infrastructure::embedded::module_source(name).is_some()
            || super::super::modules::Lookup::native_pydantic_module(name)
            || matches!(
                name,
                "typing_extensions" | "pydantic.mypy" | "string.templatelib"
            )
            || !self.provider_absent(name, target)
            || roots.is_empty()
        {
            return false;
        }
        let top = name.split('.').next().unwrap();
        for root in roots {
            if !root.is_absolute() || !catalog.absent_top_level(root, top, errors) {
                return false;
            }
        }
        for dependency in dependencies
            .iter()
            .filter(|dependency| dependency.kind == "installed-package-data")
        {
            let Some(root) = &dependency.path else {
                return false;
            };
            if !root.is_absolute() || !catalog.absent_top_level(root, top, errors) {
                return false;
            }
        }
        errors.is_empty()
    }

    fn provider_absent(&self, name: &str, target: PythonTarget) -> bool {
        let platform = if cfg!(target_os = "macos") {
            "darwin"
        } else if cfg!(target_os = "linux") {
            "linux"
        } else {
            return false;
        };
        let target = match target {
            PythonTarget::Py312 => "3.12",
            PythonTarget::Py313 => "3.13",
            PythonTarget::Py314 => "3.14",
        };
        let standard = self.standard["platforms"][platform][target].as_object();
        let child = self.children["platforms"][platform][target]["module"].as_str();
        standard.is_some_and(|modules| !modules.contains_key(name))
            && child.is_some_and(|module| module != name)
    }
}

pub(super) struct Found {
    pub sources: Vec<(PathBuf, String)>,
    pub untyped: bool,
}

pub(super) fn sources(
    dependencies: &[DependencySource],
    name: &str,
    catalog: &mut Catalog,
    errors: &mut BTreeSet<String>,
) -> Found {
    let parts: Vec<_> = name.split('.').filter(|part| !part.is_empty()).collect();
    if parts.is_empty() {
        return Found {
            sources: Vec::new(),
            untyped: false,
        };
    }
    let roots: Vec<_> = dependencies
        .iter()
        .filter(|dependency| dependency.kind == "installed-package-data")
        .filter_map(|dependency| dependency.path.as_ref())
        .collect();
    for root in roots {
        let stub = root.join(format!("{}-stubs", parts[0]));
        let stub_exists = catalog.is_dir(&stub, errors);
        if !stub_exists && catalog.is_file(&root.join(format!("{}.py", parts[0])), errors) {
            return Found {
                sources: Vec::new(),
                untyped: true,
            };
        }
        let partial = match fs::read_to_string(stub.join("py.typed")) {
            Ok(marker) => marker.lines().any(|line| line.trim() == "partial"),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
            Err(error) => {
                errors.insert(format!("{}: {error}", stub.join("py.typed").display()));
                false
            }
        };
        let directory = if stub_exists {
            stub
        } else {
            root.join(parts[0])
        };
        if !catalog.is_dir(&directory, errors) {
            continue;
        }
        let mut typed = directory.ends_with(format!("{}-stubs", parts[0]))
            || directory.join("py.typed").is_file();
        let mut current = directory;
        let mut selected = Vec::new();
        let mut module = parts[0].to_owned();
        let mut fallback = root.join(parts[0]);
        for (index, part) in parts.iter().enumerate() {
            if index > 0 {
                current = current.join(part);
                fallback = fallback.join(part);
                module.push('.');
                module.push_str(part);
                typed |= current.join("py.typed").is_file();
            }
            let candidates = [
                current.join("__init__.pyi"),
                current.join("__init__.py"),
                current.with_extension("pyi"),
                current.with_extension("py"),
            ];
            let fallback_candidates = [
                fallback.join("__init__.pyi"),
                fallback.join("__init__.py"),
                fallback.with_extension("pyi"),
                fallback.with_extension("py"),
            ];
            let selected_path = candidates
                .iter()
                .enumerate()
                .find_map(|(index, path)| {
                    (catalog.is_file(path, errors)
                        && (index >= 2 || catalog.has_exact_name(&current, errors)))
                    .then_some(path)
                })
                .or_else(|| {
                    partial
                        .then(|| {
                            fallback_candidates
                                .iter()
                                .enumerate()
                                .find_map(|(index, path)| {
                                    (catalog.is_file(path, errors)
                                        && (index >= 2
                                            || catalog.has_exact_name(&fallback, errors)))
                                    .then_some(path)
                                })
                        })
                        .flatten()
                });
            if let Some(path) = selected_path {
                match path.canonicalize() {
                    Ok(path) => selected.push((path, module.clone())),
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
        if typed {
            return Found {
                sources: selected,
                untyped: false,
            };
        }
        return Found {
            sources: Vec::new(),
            untyped: true,
        };
    }
    Found {
        sources: Vec::new(),
        untyped: false,
    }
}
