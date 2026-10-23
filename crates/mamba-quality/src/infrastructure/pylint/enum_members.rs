//! Enum transforms retain the first local statement and annotation-only boundary.
use mamba_language::application::quality_syntax::python_ast::{
    visitor::{walk_stmt, Visitor},
    Expr, Stmt,
};
use std::collections::BTreeMap;

#[derive(Default)]
struct Members(BTreeMap<String, bool>);
impl Members {
    fn target(&mut self, target: &Expr, has_value: bool) {
        match target {
            Expr::Name(name) => {
                self.0.entry(name.id.to_string()).or_insert(has_value);
            }
            Expr::Tuple(tuple) => {
                for target in &tuple.elts {
                    self.target(target, has_value);
                }
            }
            Expr::List(list) => {
                for target in &list.elts {
                    self.target(target, has_value);
                }
            }
            _ => {}
        }
    }
}
impl<'a> Visitor<'a> for Members {
    fn visit_stmt(&mut self, statement: &'a Stmt) {
        match statement {
            Stmt::Assign(value) => {
                for target in &value.targets {
                    self.target(target, true);
                }
            }
            Stmt::AnnAssign(value) => self.target(&value.target, value.value.is_some()),
            Stmt::FunctionDef(value) => {
                self.0.insert(value.name.to_string(), false);
            }
            Stmt::ClassDef(value) => {
                self.0.insert(value.name.to_string(), false);
            }
            _ => walk_stmt(self, statement),
        }
    }
    fn visit_expr(&mut self, _: &'a Expr) {}
}
pub(super) fn collect(body: &[Stmt]) -> BTreeMap<String, bool> {
    let mut members = Members::default();
    members.visit_body(body);
    members.0.remove("_ignore_");
    members.0
}
