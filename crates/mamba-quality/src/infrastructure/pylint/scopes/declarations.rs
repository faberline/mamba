//! Pylint keeps all assignment positions and nested global bindings.
use crate::infrastructure::project::symbols::{Definition, Symbols};
use mamba_language::application::quality_syntax::python_ast::{
    visitor::{walk_expr, walk_stmt, Visitor},
    ExceptHandler, Expr, Stmt,
};
use ruff_text_size::{Ranged, TextRange};
use std::collections::BTreeMap;

#[derive(Default)]
struct Assignments(BTreeMap<String, TextRange>);
impl Assignments {
    fn add(&mut self, name: &str, range: TextRange) {
        self.0.insert(name.into(), range);
    }
}
impl<'a> Visitor<'a> for Assignments {
    fn visit_stmt(&mut self, statement: &'a Stmt) {
        match statement {
            Stmt::FunctionDef(value) => self.add(value.name.as_str(), value.name.range()),
            Stmt::ClassDef(value) => self.add(value.name.as_str(), value.name.range()),
            Stmt::Import(_) | Stmt::ImportFrom(_) => {}
            _ => walk_stmt(self, statement),
        }
    }
    fn visit_expr(&mut self, value: &'a Expr) {
        match value {
            Expr::Name(value) if value.ctx.is_store() || value.ctx.is_del() => {
                self.add(value.id.as_str(), value.range());
            }
            Expr::Lambda(_)
            | Expr::ListComp(_)
            | Expr::SetComp(_)
            | Expr::DictComp(_)
            | Expr::Generator(_) => {}
            _ => walk_expr(self, value),
        }
    }
    fn visit_except_handler(&mut self, value: &'a ExceptHandler) {
        let ExceptHandler::ExceptHandler(value) = value;
        if let Some(name) = &value.name {
            self.add(name.as_str(), name.range());
        }
        self.visit_body(&value.body);
    }
}
pub(super) fn assignments(body: &[Stmt]) -> BTreeMap<String, TextRange> {
    let mut data = Assignments::default();
    data.visit_body(body);
    data.0
}

#[derive(Default)]
struct GlobalPositions(BTreeMap<String, TextRange>);
impl<'a> Visitor<'a> for GlobalPositions {
    fn visit_stmt(&mut self, value: &'a Stmt) {
        match value {
            Stmt::Global(value) => {
                for name in &value.names {
                    self.0.entry(name.to_string()).or_insert(value.range());
                }
            }
            Stmt::FunctionDef(_) | Stmt::ClassDef(_) => {}
            _ => walk_stmt(self, value),
        }
    }
}
pub(super) fn global_positions(body: &[Stmt]) -> BTreeMap<String, TextRange> {
    let mut data = GlobalPositions::default();
    data.visit_body(body);
    data.0
}

struct Globals<'a> {
    current: &'a str,
    is_package: bool,
    bindings: BTreeMap<String, Definition>,
}
impl<'a> Visitor<'a> for Globals<'_> {
    fn visit_stmt(&mut self, statement: &'a Stmt) {
        let body = match statement {
            Stmt::FunctionDef(value) => Some(value.body.as_slice()),
            Stmt::ClassDef(value) => Some(value.body.as_slice()),
            _ => None,
        };
        if let Some(body) = body {
            let symbols = Symbols::collect(body, self.current, self.is_package);
            for name in &symbols.globals {
                if let Some(definition) = symbols.bindings.get(name) {
                    self.bindings
                        .entry(name.clone())
                        .or_insert_with(|| definition.clone());
                }
            }
        }
        walk_stmt(self, statement);
    }
}
pub(super) fn globals(
    body: &[Stmt],
    current: &str,
    is_package: bool,
) -> BTreeMap<String, Definition> {
    let mut data = Globals {
        current,
        is_package,
        bindings: BTreeMap::new(),
    };
    data.visit_body(body);
    data.bindings
}
