//! Boolean simplification retains the immediate test-parent rule.
use mamba_language::application::quality_syntax::python_ast::{
    visitor::{walk_comprehension, walk_expr, walk_stmt, Visitor},
    Comprehension, Expr, Stmt,
};
use ruff_text_size::Ranged;
use std::collections::BTreeSet;

#[derive(Default)]
struct Conditions(BTreeSet<(u32, u32)>);
impl Conditions {
    fn condition(&mut self, value: &Expr) {
        if matches!(value, Expr::BoolOp(_)) {
            self.0
                .insert((value.start().to_u32(), value.end().to_u32()));
        }
    }
}
impl<'a> Visitor<'a> for Conditions {
    fn visit_stmt(&mut self, value: &'a Stmt) {
        match value {
            Stmt::If(value) => {
                self.condition(&value.test);
                for clause in &value.elif_else_clauses {
                    if let Some(test) = &clause.test {
                        self.condition(test);
                    }
                }
            }
            Stmt::While(value) => self.condition(&value.test),
            Stmt::Assert(value) => self.condition(&value.test),
            _ => {}
        }
        walk_stmt(self, value);
    }
    fn visit_expr(&mut self, value: &'a Expr) {
        match value {
            Expr::If(value) => self.condition(&value.test),
            Expr::Call(call) if matches!(call.func.as_ref(), Expr::Name(name) if name.id.as_str() == "bool") => {
                for value in &call.arguments.args {
                    self.condition(value);
                }
            }
            _ => {}
        }
        walk_expr(self, value);
    }
    fn visit_comprehension(&mut self, value: &'a Comprehension) {
        for condition in &value.ifs {
            self.condition(condition);
        }
        walk_comprehension(self, value);
    }
}
pub(super) fn conditions(body: &[Stmt]) -> BTreeSet<(u32, u32)> {
    let mut conditions = Conditions::default();
    conditions.visit_body(body);
    conditions.0
}
