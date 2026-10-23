//! Generator context values retain call arguments and source yield expressions.
use super::{
    callables::Target, context_values::Inferred, operand_inference::Operand, values::Value, Checker,
};
use mamba_language::application::quality_syntax::python_ast::{
    visitor::{walk_expr, walk_stmt, Visitor},
    Expr, Stmt, WithItem,
};
use std::rc::Rc;

#[derive(Default)]
pub(super) struct Yields<'a>(pub Vec<&'a Expr>);
impl<'a> Visitor<'a> for Yields<'a> {
    fn visit_stmt(&mut self, statement: &'a Stmt) {
        if !matches!(statement, Stmt::FunctionDef(_) | Stmt::ClassDef(_)) {
            walk_stmt(self, statement);
        }
    }
    fn visit_expr(&mut self, value: &'a Expr) {
        if let Expr::Yield(value) = value {
            if let Some(value) = &value.value {
                self.0.push(value);
            }
        }
        if !matches!(value, Expr::Lambda(_)) {
            walk_expr(self, value);
        }
    }
}
impl Checker<'_> {
    pub(super) fn bind_context_value(&mut self, item: &WithItem) {
        let Some(Expr::Name(name)) = item.optional_vars.as_deref() else {
            return;
        };
        let Some(Inferred::Operand(operand)) = self.infer_context_yield(&item.context_expr) else {
            return;
        };
        let target = match operand {
            Operand::Literal(name) => Target::Value(Rc::new(Value::Scalar(name))),
            Operand::Instance(class, origin) => Target::Instance(class, origin),
            Operand::Class(class, origin) => {
                Target::Contextual(Box::new(Target::Class(class)), origin)
            }
            _ => return,
        };
        self.scopes.receiver(name.id.as_str(), target);
    }
}
