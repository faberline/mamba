//! Import dependencies provide syntax facts without adding checked files.
mod module_names;
mod search;
use crate::{
    domain::{ports::ProjectStore, source::QualityWorkspace},
    infrastructure::{
        filesystem::Filesystem,
        project::{imports, CachedDocument, ProjectIndex},
    },
};
use mamba_language::application::quality_syntax::{
    self,
    python_ast::{
        visitor::{walk_stmt, Visitor},
        Stmt,
    },
};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

pub(super) struct Loaded {
    pub workspace: QualityWorkspace,
    pub read_errors: Vec<String>,
    failed_modules: BTreeSet<(usize, String)>,
    names: BTreeMap<usize, String>,
    documents: Vec<CachedDocument>,
}

impl Loaded {
    pub(super) fn index(&self) -> ProjectIndex<'_> {
        let mut index = module_names::index(&self.workspace, &self.names, &self.documents);
        index
            .namespaces
            .retain(|key| !self.failed_modules.contains(key));
        index
    }

    pub(super) fn selected_index<'a>(&self, workspace: &'a QualityWorkspace) -> ProjectIndex<'a> {
        // Selected snapshots are the unchanged prefix of the expanded workspace.
        ProjectIndex::with_documents(workspace, &self.documents[..workspace.sources.len()])
    }
}

fn ensure_documents(workspace: &QualityWorkspace, documents: &mut Vec<CachedDocument>) {
    while documents.len() < workspace.sources.len() {
        let source = &workspace.sources[documents.len()];
        let document = if source.decoding_issue.is_none() {
            Some(
                quality_syntax::parse_data(
                    &source.text,
                    crate::infrastructure::native::target(workspace, source),
                    source.is_stub,
                )
                .map(Arc::new),
            )
        } else {
            None
        };
        documents.push(document);
    }
}

struct Imports<'a> {
    module: &'a str,
    package: bool,
    names: BTreeSet<String>,
}

impl<'ast> Visitor<'ast> for Imports<'_> {
    fn visit_stmt(&mut self, statement: &'ast Stmt) {
        for import in imports(std::slice::from_ref(statement), self.module, self.package) {
            self.names.insert(import.module.clone());
            if let Some(member) = import.member {
                if member != "*" {
                    self.names.insert(format!("{}.{member}", import.module));
                }
            }
        }
        walk_stmt(self, statement);
    }

    fn visit_expr(
        &mut self,
        expression: &'ast mamba_language::application::quality_syntax::python_ast::Expr,
    ) {
        if let mamba_language::application::quality_syntax::python_ast::Expr::Attribute(_) =
            expression
        {
            let mut parts = Vec::new();
            let mut current = expression;
            while let mamba_language::application::quality_syntax::python_ast::Expr::Attribute(
                attribute,
            ) = current
            {
                parts.push(attribute.attr.as_str());
                current = &attribute.value;
            }
            if let mamba_language::application::quality_syntax::python_ast::Expr::Name(name) =
                current
            {
                if self.names.contains(name.id.as_str()) {
                    parts.push(name.id.as_str());
                    parts.reverse();
                    self.names.insert(parts.join("."));
                }
            }
        }
        mamba_language::application::quality_syntax::python_ast::visitor::walk_expr(
            self, expression,
        );
    }
}

pub(super) fn load(workspace: &QualityWorkspace) -> Loaded {
    let mut expanded = workspace.clone();
    let mut names = BTreeMap::new();
    let mut read_errors = BTreeSet::new();
    let mut failed_modules = BTreeSet::new();
    let mut documents = Vec::new();
    module_names::scanned(workspace, &mut names);
    let mut scanned = 0;
    loop {
        let mut pending = BTreeSet::new();
        ensure_documents(&expanded, &mut documents);
        let index = module_names::index(&expanded, &names, &documents);
        let mut known: BTreeSet<_> = expanded
            .sources
            .iter()
            .enumerate()
            .map(|(id, source)| {
                (
                    source.project,
                    source.path.clone(),
                    index.modules[id].name.clone(),
                )
            })
            .collect();
        for (id, source) in expanded.sources.iter().enumerate().skip(scanned) {
            let module = &index.modules[id];
            let Some(document) = &module.document else {
                continue;
            };
            let mut imports = Imports {
                module: &module.name,
                package: module.is_package,
                names: BTreeSet::new(),
            };
            imports.visit_body(&document.parsed.syntax().body);
            for name in imports.names {
                if index.names.contains_key(&(source.project, name.clone())) {
                    continue;
                }
                for (path, module) in search::sources(
                    &expanded.projects[source.project].dependencies,
                    &name,
                    &mut read_errors,
                ) {
                    if known.insert((source.project, path.clone(), module.clone())) {
                        pending.insert((source.project, path, module));
                    }
                }
            }
        }
        scanned = expanded.sources.len();
        drop(index);
        if pending.is_empty() {
            break;
        }
        for (project, path, module) in pending {
            let loaded = Filesystem.load(&[path], true);
            if loaded.sources.is_empty()
                && loaded
                    .issues
                    .iter()
                    .any(|issue| issue.code == "source-read" || issue.code == "path-read")
            {
                failed_modules.insert((project, module.clone()));
            }
            for issue in &loaded.issues {
                if issue.code == "source-read" || issue.code == "path-read" {
                    read_errors.insert(format!(
                        "{}: {}",
                        issue
                            .path
                            .as_deref()
                            .unwrap_or(&workspace.projects[project].root)
                            .display(),
                        issue.message
                    ));
                }
            }
            for mut source in loaded.sources {
                source.project = project;
                let id = expanded.sources.len();
                names.insert(id, module.clone());
                expanded.sources.push(source);
            }
        }
    }
    Loaded {
        workspace: expanded,
        read_errors: read_errors.into_iter().collect(),
        failed_modules,
        names,
        documents,
    }
}
