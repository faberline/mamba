//! Named expressions and delete targets retain their enclosing lexical bindings.
use mamba_language::application::quality_syntax::python_ast::{
    visitor::{walk_expr, walk_stmt, Visitor},
    Expr, ExprContext, Stmt,
};
use ruff_text_size::{Ranged, TextRange};

#[derive(Default)]
struct Collector(Vec<(String, TextRange)>);
impl<'a> Visitor<'a> for Collector {
    fn visit_stmt(&mut self, statement: &'a Stmt) {
        if !matches!(statement, Stmt::FunctionDef(_) | Stmt::ClassDef(_)) {
            walk_stmt(self, statement);
        }
    }
    fn visit_expr(&mut self, expression: &'a Expr) {
        if let Expr::Name(name) = expression {
            if name.ctx == ExprContext::Del {
                self.0.push((name.id.to_string(), name.range()));
            }
        }
        if let Expr::Named(named) = expression {
            if let Expr::Name(name) = named.target.as_ref() {
                self.0.push((name.id.to_string(), name.range()));
            }
        }
        if !matches!(expression, Expr::Lambda(_)) {
            walk_expr(self, expression);
        }
    }
}
pub(super) fn collect(body: &[Stmt]) -> Vec<(String, TextRange)> {
    let mut collector = Collector::default();
    collector.visit_body(body);
    collector.0
}
