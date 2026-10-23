//! Await diagnostics use the nearest function or lambda scope.
use mamba_language::application::quality_syntax::python_ast::{
    visitor::{walk_expr, walk_stmt, Visitor},
    Expr, Stmt,
};
use ruff_text_size::Ranged;
use std::collections::BTreeSet;

#[derive(Default)]
struct Contexts {
    asynchronous: bool,
    invalid: BTreeSet<u32>,
}
impl<'a> Visitor<'a> for Contexts {
    fn visit_stmt(&mut self, statement: &'a Stmt) {
        if let Stmt::FunctionDef(function) = statement {
            let previous = self.asynchronous;
            self.asynchronous = function.is_async;
            walk_stmt(self, statement);
            self.asynchronous = previous;
        } else {
            walk_stmt(self, statement);
        }
    }
    fn visit_expr(&mut self, expression: &'a Expr) {
        if matches!(expression, Expr::Await(_)) && !self.asynchronous {
            self.invalid.insert(expression.start().to_u32());
        }
        let previous = self.asynchronous;
        if matches!(expression, Expr::Lambda(_)) {
            self.asynchronous = false;
        }
        walk_expr(self, expression);
        self.asynchronous = previous;
    }
}
pub(super) fn invalid_awaits(body: &[Stmt]) -> BTreeSet<u32> {
    let mut contexts = Contexts::default();
    contexts.visit_body(body);
    contexts.invalid
}
