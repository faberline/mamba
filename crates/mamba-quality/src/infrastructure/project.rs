//! Shared source syntax and import relationships contain no tool-specific types.
use crate::domain::source::QualityWorkspace;
use mamba_language::application::quality_syntax::{
    self,
    python_ast::{
        visitor::{walk_stmt, Visitor},
        Stmt,
    },
    SyntaxDocument,
};
use ruff_text_size::{Ranged, TextRange};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
    sync::Arc,
};
mod namespace_names;
pub(crate) mod symbols;

pub(crate) type ModuleId = usize;
pub(crate) type CachedDocument = Option<Result<Arc<quality_syntax::SyntaxDocumentData>, String>>;

#[derive(Clone)]
pub(crate) struct Import {
    pub local: String,
    pub module: String,
    pub member: Option<String>,
    pub alias: Option<String>,
    pub explicit: bool,
    pub range: TextRange,
}

pub(crate) struct Module<'a> {
    pub name: String,
    pub project: usize,
    pub is_package: bool,
    pub document: Option<SyntaxDocument<'a>>,
}

pub(crate) struct ProjectIndex<'a> {
    pub modules: Vec<Module<'a>>,
    pub names: BTreeMap<(usize, String), ModuleId>,
    pub dependents: Vec<BTreeSet<ModuleId>>,
    pub namespaces: BTreeSet<(usize, String)>,
    pub order: Vec<ModuleId>,
}

impl<'a> ProjectIndex<'a> {
    #[cfg(test)]
    pub(crate) fn new(workspace: &'a QualityWorkspace) -> Self {
        Self::build_with_documents(workspace, false, None)
    }

    pub(crate) fn with_documents(
        workspace: &'a QualityWorkspace,
        documents: &[CachedDocument],
    ) -> Self {
        Self::build_with_documents(workspace, false, Some(documents))
    }

    pub(crate) fn for_type_inference_with_documents(
        workspace: &'a QualityWorkspace,
        documents: &[CachedDocument],
    ) -> Self {
        Self::build_with_documents(workspace, true, Some(documents))
    }

    fn build_with_documents(
        workspace: &'a QualityWorkspace,
        type_defaults: bool,
        documents: Option<&[CachedDocument]>,
    ) -> Self {
        let package_directories: BTreeSet<_> = workspace
            .sources
            .iter()
            .filter(|source| {
                matches!(
                    source.path.file_name().and_then(|name| name.to_str()),
                    Some("__init__.py" | "__init__.pyi")
                )
            })
            .map(|source| source.path.parent().unwrap().to_path_buf())
            .collect();
        let mut modules = Vec::new();
        let mut names = BTreeMap::new();
        for (id, source) in workspace.sources.iter().enumerate() {
            let root = &workspace.projects[source.project].root;
            let name = module_name(&source.path, root, &package_directories, type_defaults);
            let is_package = source
                .path
                .file_stem()
                .is_some_and(|name| name == "__init__");
            let document = if source.decoding_issue.is_none() {
                let parsed = match documents.and_then(|documents| documents.get(id)) {
                    Some(Some(Ok(data))) => {
                        Some(SyntaxDocument::from_data(&source.text, Arc::clone(data)))
                    }
                    Some(Some(Err(_))) => None,
                    Some(None) => None,
                    None => quality_syntax::parse(
                        &source.text,
                        super::native::target(workspace, source),
                        source.is_stub,
                    )
                    .ok(),
                };
                parsed.filter(|document| {
                    document.is_valid()
                        || (type_defaults && document.has_only_type_inference_version_errors())
                })
            } else {
                None
            };
            let key = (source.project, name.clone());
            if source.is_stub || !names.contains_key(&key) {
                names.insert(key, id);
            }
            modules.push(Module {
                name,
                project: source.project,
                is_package,
                document,
            });
        }
        if type_defaults {
            namespace_names::qualify(workspace, &mut modules);
            names.clear();
            for (id, (source, module)) in workspace.sources.iter().zip(&modules).enumerate() {
                let key = (module.project, module.name.clone());
                if source.is_stub || !names.contains_key(&key) {
                    names.insert(key, id);
                }
            }
        }
        let mut namespaces = BTreeSet::new();
        for (project, name) in names.keys() {
            namespaces.insert((*project, name.clone()));
            for (offset, _) in name.match_indices('.') {
                namespaces.insert((*project, name[..offset].into()));
            }
        }
        let mut dependents = vec![BTreeSet::new(); modules.len()];
        let mut dependencies = vec![BTreeSet::new(); modules.len()];
        for (id, module) in modules.iter().enumerate() {
            if let Some(document) = &module.document {
                let mut collector = ImportCollector {
                    current: &module.name,
                    is_package: module.is_package,
                    imports: Vec::new(),
                };
                for statement in &document.parsed.syntax().body {
                    collector.visit_stmt(statement);
                }
                for import in collector.imports {
                    if let Some(&dependency) = names.get(&(module.project, import.module)) {
                        dependents[dependency].insert(id);
                        dependencies[id].insert(dependency);
                    }
                }
            }
        }
        let mut visited = vec![0; modules.len()];
        let mut order = Vec::new();
        for id in 0..modules.len() {
            order_module(id, &dependencies, &mut visited, &mut order);
        }
        Self {
            modules,
            names,
            dependents,
            namespaces,
            order,
        }
    }
}

fn order_module(
    id: ModuleId,
    dependencies: &[BTreeSet<ModuleId>],
    visited: &mut [u8],
    order: &mut Vec<ModuleId>,
) {
    let mut stack = vec![(id, false)];
    while let Some((id, expanded)) = stack.pop() {
        if expanded {
            visited[id] = 2;
            order.push(id);
            continue;
        }
        if visited[id] != 0 {
            continue;
        }
        visited[id] = 1;
        stack.push((id, true));
        stack.extend(dependencies[id].iter().rev().copied().map(|id| (id, false)));
    }
}

struct ImportCollector<'a> {
    current: &'a str,
    is_package: bool,
    imports: Vec<Import>,
}

impl<'a> Visitor<'a> for ImportCollector<'_> {
    fn visit_stmt(&mut self, statement: &'a Stmt) {
        self.imports.extend(imports(
            std::slice::from_ref(statement),
            self.current,
            self.is_package,
        ));
        walk_stmt(self, statement);
    }
}

fn module_name(
    path: &Path,
    root: &Path,
    packages: &BTreeSet<std::path::PathBuf>,
    namespace_fallback: bool,
) -> String {
    let parent = path.parent().unwrap_or(root);
    let highest = parent
        .ancestors()
        .take_while(|directory| directory.starts_with(root))
        .filter(|directory| packages.contains(*directory))
        .last();
    let base = highest
        .and_then(Path::parent)
        .unwrap_or(if namespace_fallback { root } else { parent });
    let relative = path.strip_prefix(base).unwrap_or(path);
    let mut parts: Vec<_> = relative
        .parent()
        .into_iter()
        .flat_map(Path::iter)
        .map(|part| part.to_string_lossy().into_owned())
        .collect();
    let stem = path.file_stem().unwrap().to_string_lossy();
    if stem != "__init__" {
        parts.push(stem.into_owned());
    }
    if parts.is_empty() {
        parts.push(parent.file_name().unwrap().to_string_lossy().into_owned());
    }
    parts.join(".")
}

pub(crate) fn imports(body: &[Stmt], current: &str, is_package: bool) -> Vec<Import> {
    let mut imports = Vec::new();
    for statement in body {
        match statement {
            Stmt::Import(statement) => {
                for alias in &statement.names {
                    let local = alias
                        .asname
                        .as_ref()
                        .map(ToString::to_string)
                        .unwrap_or_else(|| alias.name.as_str().split('.').next().unwrap().into());
                    imports.push(Import {
                        local,
                        module: alias.name.to_string(),
                        member: None,
                        alias: alias.asname.as_ref().map(ToString::to_string),
                        explicit: alias
                            .asname
                            .as_ref()
                            .is_some_and(|name| name.as_str() == alias.name.as_str()),
                        range: statement.range(),
                    });
                }
            }
            Stmt::ImportFrom(statement) => {
                let module = relative_module(
                    current,
                    is_package,
                    statement.level,
                    statement.module.as_ref().map(|module| module.as_str()),
                );
                for alias in &statement.names {
                    imports.push(Import {
                        local: alias.asname.as_ref().unwrap_or(&alias.name).to_string(),
                        module: module.clone(),
                        member: Some(alias.name.to_string()),
                        alias: alias.asname.as_ref().map(ToString::to_string),
                        explicit: alias
                            .asname
                            .as_ref()
                            .is_some_and(|name| name.as_str() == alias.name.as_str()),
                        range: statement.range(),
                    });
                }
            }
            _ => {}
        }
    }
    imports
}

fn relative_module(current: &str, is_package: bool, level: u32, module: Option<&str>) -> String {
    if level == 0 {
        return module.unwrap_or("").into();
    }
    let mut parts: Vec<_> = current.split('.').collect();
    if !is_package {
        parts.pop();
    }
    for _ in 1..level {
        parts.pop();
    }
    if let Some(module) = module {
        parts.push(module);
    }
    parts.join(".")
}
