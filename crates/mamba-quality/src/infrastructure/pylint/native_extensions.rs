//! Opaque extension data is located by filename and is never loaded or executed.
use super::super::project::ProjectIndex;
use crate::domain::source::QualityWorkspace;
use std::{cell::RefCell, collections::BTreeMap, path::PathBuf};

#[derive(Clone, Copy)]
enum Kind {
    Extension,
    Namespace,
}

pub(super) struct Extensions {
    project_root: PathBuf,
    roots: Vec<PathBuf>,
    suffixes: Vec<String>,
    found: RefCell<BTreeMap<String, Option<Kind>>>,
}
impl Extensions {
    pub(super) fn new(
        index: &ProjectIndex<'_>,
        workspace: &QualityWorkspace,
        project: usize,
    ) -> Self {
        let info = &workspace.projects[project];
        let mut roots = vec![info.root.clone()];
        for (id, module) in index
            .modules
            .iter()
            .enumerate()
            .filter(|(_, module)| module.project == project)
        {
            let mut root = workspace.sources[id].path.parent();
            let levels = module.name.split('.').count() - usize::from(!module.is_package);
            for _ in 0..levels {
                root = root.and_then(std::path::Path::parent);
            }
            if let Some(root) = root {
                if !roots.iter().any(|value| value == root) {
                    roots.push(root.into());
                }
            }
        }
        for dependency in &info.dependencies {
            if let Some(root) = dependency.path.as_ref().filter(|root| root.is_dir()) {
                if !roots.contains(root) {
                    roots.push(root.clone());
                }
            }
        }
        let platform = if cfg!(target_os = "macos") {
            "darwin"
        } else if cfg!(target_arch = "aarch64") {
            "aarch64-linux-gnu"
        } else {
            "x86_64-linux-gnu"
        };
        Self {
            project_root: info.root.clone(),
            roots,
            suffixes: vec![
                format!(".cpython-3{}-{platform}.so", info.python_version.minor()),
                ".abi3.so".into(),
                ".so".into(),
            ],
            found: RefCell::new(BTreeMap::new()),
        }
    }

    fn kind(&self, name: &str) -> Option<Kind> {
        if let Some(value) = self.found.borrow().get(name) {
            return *value;
        }
        let relative: PathBuf = name.split('.').collect();
        let result = self.roots.iter().find_map(|root| {
            let path = root.join(&relative);
            // The original source builder prefers Python source to extensions.
            if path.with_extension("py").is_file() || path.join("__init__.py").is_file() {
                return None;
            }
            let file = path.file_name()?.to_str()?;
            let parent = path.parent()?;
            if self
                .suffixes
                .iter()
                .any(|suffix| parent.join(format!("{file}{suffix}")).is_file())
            {
                return Some(Kind::Extension);
            }
            path.is_dir().then_some(Kind::Namespace)
        });
        self.found.borrow_mut().insert(name.into(), result);
        result
    }
    pub(super) fn exists(&self, name: &str) -> bool {
        self.kind(name).is_some()
    }
    pub(super) fn contains(&self, name: &str) -> bool {
        matches!(self.kind(name), Some(Kind::Extension))
    }
    pub(super) fn namespace(&self, name: &str) -> bool {
        matches!(self.kind(name), Some(Kind::Namespace))
    }
    pub(super) fn first_party(&self, name: &str) -> bool {
        let path = self.project_root.join(name.split('.').collect::<PathBuf>());
        let (Some(file), Some(parent)) = (
            path.file_name().and_then(|value| value.to_str()),
            path.parent(),
        ) else {
            return false;
        };
        self.suffixes
            .iter()
            .any(|suffix| parent.join(format!("{file}{suffix}")).is_file())
    }
}

pub(super) fn scope_attribute(name: &str) -> bool {
    matches!(
        name,
        "__name__" | "__file__" | "__doc__" | "__package__" | "__dict__" | "__spec__"
    )
}
