//! Absolute imports keep namespace prefixes around regular inner packages.
use super::{Module, QualityWorkspace};
use mamba_language::application::quality_syntax::python_ast::{
    visitor::{walk_stmt, Visitor},
    Stmt,
};
use std::collections::BTreeSet;

pub(super) fn qualify(workspace: &QualityWorkspace, modules: &mut [Module<'_>]) {
    let mut imports = BTreeSet::new();
    for module in modules.iter() {
        let Some(document) = &module.document else {
            continue;
        };
        let mut visitor = Prefixes {
            project: module.project,
            imports: &mut imports,
        };
        for statement in &document.parsed.syntax().body {
            visitor.visit_stmt(statement);
        }
    }
    for (source, module) in workspace.sources.iter().zip(modules) {
        let root = &workspace.projects[source.project].root;
        let relative = source.path.strip_prefix(root).unwrap_or(&source.path);
        let Some(prefix) = relative.iter().next().and_then(|part| part.to_str()) else {
            continue;
        };
        if imports.contains(&(module.project, prefix.to_string())) {
            module.name = super::module_name(&source.path, root, &BTreeSet::new(), true);
        }
    }
}

struct Prefixes<'a> {
    project: usize,
    imports: &'a mut BTreeSet<(usize, String)>,
}

impl Prefixes<'_> {
    fn add(&mut self, name: &str) {
        if let Some(prefix) = name.split('.').next() {
            self.imports.insert((self.project, prefix.to_string()));
        }
    }
}

impl<'a> Visitor<'a> for Prefixes<'_> {
    fn visit_stmt(&mut self, statement: &'a Stmt) {
        match statement {
            Stmt::Import(import) => {
                for alias in &import.names {
                    self.add(&alias.name);
                }
            }
            Stmt::ImportFrom(import) if import.level == 0 => {
                if let Some(module) = &import.module {
                    self.add(module);
                }
            }
            _ => {}
        }
        walk_stmt(self, statement);
    }
}
