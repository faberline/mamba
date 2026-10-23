//! Operand binding alternatives retain conditional definitions in source order.
use mamba_language::application::quality_syntax::python_ast::{
    visitor::{walk_stmt, Visitor},
    Expr, Stmt,
};
use std::collections::BTreeMap;
#[derive(Default)]
struct Bindings {
    values: BTreeMap<String, Vec<Expr>>,
    conditional: bool,
}
impl Bindings {
    fn target(&mut self, target: &Expr, value: &Expr) {
        if let Expr::Name(name) = target {
            let values = self.values.entry(name.id.to_string()).or_default();
            if !self.conditional {
                values.clear();
            }
            values.push(value.clone());
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
            _ => {
                let previous = self.conditional;
                self.conditional = true;
                walk_stmt(self, statement);
                self.conditional = previous;
            }
        }
    }
}
pub(super) fn collect(body: &[Stmt]) -> BTreeMap<String, Vec<Expr>> {
    let mut bindings = Bindings::default();
    bindings.visit_body(body);
    bindings.values
}
