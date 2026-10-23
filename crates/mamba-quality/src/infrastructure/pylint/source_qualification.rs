//! Callable names retain lexical parents from the existing project syntax.
use super::Checker;
use mamba_language::application::quality_syntax::python_ast::{
    visitor::{walk_stmt, Visitor},
    Stmt,
};
use ruff_text_size::Ranged;

struct Finder {
    offset: u32,
    parents: Vec<String>,
    found: Option<String>,
}
impl<'a> Visitor<'a> for Finder {
    fn visit_stmt(&mut self, statement: &'a Stmt) {
        if self.found.is_some() {
            return;
        }
        let name = match statement {
            Stmt::FunctionDef(value) => Some(&value.name),
            Stmt::ClassDef(value) => Some(&value.name),
            _ => None,
        };
        if let Some(name) = name {
            self.parents.push(name.to_string());
            if name.start().to_u32() == self.offset {
                self.found = Some(self.parents.join("."));
            } else {
                walk_stmt(self, statement);
            }
            self.parents.pop();
        } else {
            walk_stmt(self, statement);
        }
    }
}
impl Checker<'_> {
    pub(super) fn source_qualification(&self, module: &str, offset: u32) -> Option<String> {
        let index = self.modules.source_index(self.source.project, module)?;
        let document = self.index.modules[index].document.as_ref()?;
        let mut finder = Finder {
            offset,
            parents: Vec::new(),
            found: None,
        };
        finder.visit_body(&document.parsed.syntax().body);
        finder.found.map(|name| format!("{module}.{name}"))
    }
}
