//! Value bindings retain source syntax within each lexical body.
use mamba_language::application::quality_syntax::python_ast::{
    visitor::{walk_stmt, Visitor},
    Expr, Stmt,
};
use std::collections::BTreeMap;

#[derive(Default)]
struct Bindings(BTreeMap<String, Expr>, bool);
impl Bindings {
    fn target(&mut self, target: &Expr, value: &Expr) {
        match target {
            Expr::Name(name) => {
                if self.1 {
                    self.0
                        .entry(name.id.to_string())
                        .or_insert_with(|| value.clone());
                } else {
                    self.0.insert(name.id.to_string(), value.clone());
                }
            }
            Expr::Tuple(tuple) => self.unpacked(&tuple.elts, value),
            Expr::List(list) => self.unpacked(&list.elts, value),
            _ => {}
        }
    }
    fn unpacked(&mut self, targets: &[Expr], value: &Expr) {
        let values = match value {
            Expr::Tuple(value) => &value.elts,
            Expr::List(value) => &value.elts,
            _ => return,
        };
        for (target, value) in targets.iter().zip(values) {
            self.target(target, value);
        }
    }
}
impl<'a> Visitor<'a> for Bindings {
    fn visit_stmt(&mut self, statement: &'a Stmt) {
        match statement {
            Stmt::Assign(value) => {
                for target in &value.targets {
                    self.target(target, &value.value);
                }
            }
            Stmt::AnnAssign(value) => {
                if let Some(expression) = &value.value {
                    self.target(&value.target, expression);
                }
            }
            Stmt::FunctionDef(_) | Stmt::ClassDef(_) => {}
            _ => walk_stmt(self, statement),
        }
    }
}
pub(super) fn collect(body: &[Stmt]) -> BTreeMap<String, Expr> {
    let mut bindings = Bindings::default();
    bindings.visit_body(body);
    bindings.0
}
pub(super) fn collect_first(body: &[Stmt]) -> BTreeMap<String, Expr> {
    let mut bindings = Bindings(BTreeMap::new(), true);
    bindings.visit_body(body);
    bindings.0
}
