//! Class checks borrow source syntax and retain enclosing declaration names.
use super::{classes::Class, Checker};
use mamba_language::application::quality_syntax::python_ast::{
    visitor::{walk_stmt, Visitor},
    Stmt, StmtClassDef,
};
use ruff_text_size::Ranged;

struct Finder<'a> {
    offset: u32,
    found: Option<&'a StmtClassDef>,
    path: Vec<String>,
    qualified: Option<String>,
}
impl<'a> Visitor<'a> for Finder<'a> {
    fn visit_stmt(&mut self, statement: &'a Stmt) {
        if self.found.is_some() {
            return;
        }
        let name = match statement {
            Stmt::ClassDef(class) => Some(class.name.to_string()),
            Stmt::FunctionDef(function) => Some(function.name.to_string()),
            _ => None,
        };
        if let Some(name) = &name {
            self.path.push(name.clone());
        }
        if let Stmt::ClassDef(class) = statement {
            if class.name.start().to_u32() == self.offset {
                self.found = Some(class);
                self.qualified = Some(self.path.join("."));
            }
        }
        if self.found.is_none() {
            walk_stmt(self, statement);
        }
        if name.is_some() {
            self.path.pop();
        }
    }
}
impl Checker<'_> {
    fn class_source(&self, class: &Class) -> Option<Finder<'_>> {
        let index = self
            .modules
            .source_index(self.source.project, &class.identity.0)?;
        let document = self.index.modules[index].document.as_ref()?;
        let mut finder = Finder {
            offset: class.identity.1,
            found: None,
            path: vec![class.identity.0.clone()],
            qualified: None,
        };
        finder.visit_body(&document.parsed.syntax().body);
        Some(finder)
    }

    pub(super) fn source_class(&self, class: &Class) -> Option<&StmtClassDef> {
        self.class_source(class)?.found
    }

    pub(super) fn source_class_qualified_name(&self, class: &Class) -> Option<String> {
        self.class_source(class)?.qualified
    }
}
