//! Methods in conditional suites remain class declarations.
use mamba_language::application::quality_syntax::python_ast::{
    visitor::{walk_stmt, Visitor},
    Stmt, StmtFunctionDef,
};

#[derive(Default)]
struct Collector<'a>(Vec<&'a StmtFunctionDef>);
impl<'a> Visitor<'a> for Collector<'a> {
    fn visit_stmt(&mut self, value: &'a Stmt) {
        match value {
            Stmt::FunctionDef(value) => self.0.push(value),
            Stmt::ClassDef(_) => {}
            _ => walk_stmt(self, value),
        }
    }
}
pub(in super::super) fn methods(body: &[Stmt]) -> Vec<&StmtFunctionDef> {
    let mut data = Collector::default();
    data.visit_body(body);
    data.0
}
