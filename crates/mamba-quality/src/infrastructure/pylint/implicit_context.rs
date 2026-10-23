//! Boolean contexts retain the original distinct call and comparison parent rules.
use mamba_language::application::quality_syntax::python_ast::{
    visitor::{walk_expr, walk_stmt, Visitor},
    Comprehension, Expr, Stmt, UnaryOp,
};
use ruff_text_size::Ranged;
use std::collections::BTreeSet;

#[derive(Default)]
pub(super) struct Conditions {
    pub calls: BTreeSet<u32>,
    pub comparisons: BTreeSet<u32>,
}
impl Conditions {
    fn condition(&mut self, value: &Expr, calls: bool) {
        match value {
            Expr::BoolOp(value) => {
                for value in &value.values {
                    self.condition(value, calls);
                }
            }
            Expr::Compare(value) => {
                self.comparisons.insert(value.start().to_u32());
            }
            Expr::Call(value) if calls => {
                self.calls.insert(value.start().to_u32());
            }
            _ => {}
        }
    }
}
impl<'a> Visitor<'a> for Conditions {
    fn visit_stmt(&mut self, value: &'a Stmt) {
        match value {
            Stmt::If(value) => {
                self.condition(&value.test, true);
                for clause in &value.elif_else_clauses {
                    if let Some(test) = &clause.test {
                        self.condition(test, true);
                    }
                }
            }
            Stmt::While(value) => self.condition(&value.test, true),
            Stmt::Assert(value) => self.condition(&value.test, true),
            _ => {}
        }
        walk_stmt(self, value);
    }
    fn visit_expr(&mut self, value: &'a Expr) {
        match value {
            Expr::If(value) => self.condition(&value.test, true),
            Expr::UnaryOp(value) if value.op == UnaryOp::Not => {
                self.condition(&value.operand, false)
            }
            Expr::Call(call) => {
                if let Expr::Name(name) = call.func.as_ref() {
                    for argument in &call.arguments.args {
                        match (name.id.as_str(), argument) {
                            ("bool", value) => self.condition(value, true),
                            ("all" | "any", Expr::Generator(value)) => {
                                self.condition(&value.elt, false)
                            }
                            ("filter", Expr::Lambda(value)) => self.condition(&value.body, false),
                            _ => {}
                        }
                    }
                }
            }
            _ => {}
        }
        walk_expr(self, value);
    }
    fn visit_comprehension(&mut self, value: &'a Comprehension) {
        for condition in &value.ifs {
            self.condition(condition, false);
            if matches!(condition, Expr::Call(_)) {
                self.calls.insert(condition.start().to_u32());
            }
        }
        mamba_language::application::quality_syntax::python_ast::visitor::walk_comprehension(
            self, value,
        );
    }
}
pub(super) fn conditions(body: &[Stmt]) -> Conditions {
    let mut conditions = Conditions::default();
    conditions.visit_body(body);
    conditions
}
