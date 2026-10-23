//! Boolean-use metadata retains each comparison's immediate AST parent.
use mamba_language::application::quality_syntax::python_ast::{
    visitor::{walk_comprehension, walk_expr, walk_stmt, Visitor},
    Comprehension, Expr, Stmt,
};
use ruff_text_size::Ranged;
use std::collections::BTreeSet;

#[derive(Default)]
struct Conditions(BTreeSet<u32>);
impl Conditions {
    fn condition(&mut self, expression: &Expr) {
        if matches!(expression, Expr::Compare(_)) {
            self.0.insert(expression.start().to_u32());
        }
    }
}
impl<'a> Visitor<'a> for Conditions {
    fn visit_stmt(&mut self, statement: &'a Stmt) {
        match statement {
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
        walk_stmt(self, statement);
    }
    fn visit_expr(&mut self, expression: &'a Expr) {
        match expression {
            Expr::If(value) => self.condition(&value.test),
            Expr::Call(call) if matches!(call.func.as_ref(), Expr::Name(name) if name.id.as_str() == "bool") => {
                for argument in &call.arguments.args {
                    self.condition(argument);
                }
            }
            _ => {}
        }
        walk_expr(self, expression);
    }
    fn visit_comprehension(&mut self, comprehension: &'a Comprehension) {
        for condition in &comprehension.ifs {
            self.condition(condition);
        }
        walk_comprehension(self, comprehension);
    }
}
pub(super) fn conditions(body: &[Stmt]) -> BTreeSet<u32> {
    let mut conditions = Conditions::default();
    conditions.visit_body(body);
    conditions.0
}
