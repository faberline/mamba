//! Partial cursor consumption skips breaks in deeper loops and nested declarations.
use mamba_language::application::quality_syntax::python_ast::{
    visitor::{walk_stmt, Visitor},
    Stmt, StmtFor,
};
#[derive(Default)]
struct Scan {
    loops: usize,
    found: bool,
}
impl<'a> Visitor<'a> for Scan {
    fn visit_stmt(&mut self, statement: &'a Stmt) {
        match statement {
            Stmt::FunctionDef(_) | Stmt::ClassDef(_) => return,
            Stmt::Return(_) => self.found = true,
            Stmt::Break(_) if self.loops == 0 => self.found = true,
            Stmt::For(_) | Stmt::While(_) => {
                self.loops += 1;
                walk_stmt(self, statement);
                self.loops -= 1;
                return;
            }
            _ => {}
        }
        walk_stmt(self, statement);
    }
}
pub(super) fn early(loop_: &StmtFor) -> bool {
    let mut scan = Scan::default();
    scan.visit_body(&loop_.body);
    scan.visit_body(&loop_.orelse);
    scan.found
        || loop_
            .orelse
            .iter()
            .any(|statement| matches!(statement, Stmt::Return(_) | Stmt::Raise(_) | Stmt::Break(_)))
}
