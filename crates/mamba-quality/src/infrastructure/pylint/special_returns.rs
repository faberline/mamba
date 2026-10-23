//! Special result protocols retain original ambiguity and function-body boundaries.
use super::{
    callables::{Signature, Target},
    classes::MethodKind,
    scopes::Kind,
    values::Value,
    Checker,
};
use mamba_language::application::quality_syntax::python_ast::{
    visitor::{walk_stmt, Visitor},
    Expr, Stmt, StmtFunctionDef, UnaryOp,
};
use ruff_text_size::Ranged;
mod inference;
mod protocols;
use inference::ResultValue;

#[derive(Default)]
struct Returns<'a> {
    values: Vec<Option<&'a Expr>>,
    generator: bool,
}
impl<'a> Visitor<'a> for Returns<'a> {
    fn visit_stmt(&mut self, statement: &'a Stmt) {
        match statement {
            Stmt::Return(value) => self.values.push(value.value.as_deref()),
            Stmt::FunctionDef(_) | Stmt::ClassDef(_) => {}
            _ => walk_stmt(self, statement),
        }
    }
    fn visit_expr(&mut self, value: &'a Expr) {
        if matches!(value, Expr::Yield(_) | Expr::YieldFrom(_)) {
            self.generator = true;
        }
        if !matches!(value, Expr::Lambda(_)) {
            mamba_language::application::quality_syntax::python_ast::visitor::walk_expr(
                self, value,
            );
        }
    }
}

impl Checker<'_> {
    fn invalid_newargs(&self, value: &ResultValue) -> bool {
        let ResultValue::Value(Value::Sequence(values, "tuple")) = value else {
            return false;
        };
        values.len() != 2
            || values
                .iter()
                .zip(["tuple", "dict"])
                .any(|(value, expected)| {
                    !matches!(value, Value::Unknown)
                        && !self.protocol_accepts(&ResultValue::Value(value.clone()), expected)
                })
    }
    pub(super) fn check_special_return(&mut self, function: &StmtFunctionDef) {
        let Some(protocol) = protocols::lookup(function.name.as_str()) else {
            return;
        };
        let mut returns = Returns::default();
        returns.visit_body(&function.body);
        if !returns.generator && returns.values.len() > 1 {
            return;
        }
        let body = &function.body[usize::from(super::docstring(&function.body))..];
        if matches!(body, [Stmt::Expr(value)] if matches!(value.value.as_ref(), Expr::EllipsisLiteral(_)))
        {
            return;
        }
        let signature = Signature::from_function(function);
        if returns.values.is_empty()
            && !returns.generator
            && (signature.pass_body
                || signature.abstract_body
                || body.iter().any(|value| matches!(value, Stmt::Raise(_))))
        {
            return;
        }
        let Some((class, origin)) = self.class_stack.last().cloned() else {
            return;
        };
        let kind = class
            .methods
            .get(function.name.as_str())
            .map_or(MethodKind::Instance, |value| value.kind);
        self.scopes
            .enter(Kind::Function, &function.body, Some(&function.parameters));
        if kind != MethodKind::Static {
            if let Some(first) = function.parameters.iter().next() {
                self.scopes.receiver(
                    first.name().as_str(),
                    if kind == MethodKind::Class {
                        Target::Contextual(Box::new(Target::Class(class)), origin)
                    } else {
                        Target::Instance(class, origin)
                    },
                );
            }
        }
        let value = if returns.generator {
            ResultValue::Iterator
        } else {
            returns
                .values
                .first()
                .copied()
                .flatten()
                .map_or(ResultValue::Value(Value::Scalar("NoneType")), |value| {
                    self.protocol_result(value)
                })
        };
        let negative = returns.values.first().copied().flatten().is_some_and(|value| matches!(value, Expr::UnaryOp(value) if value.op == UnaryOp::USub && matches!(value.operand.as_ref(), Expr::NumberLiteral(value) if matches!(&value.value, mamba_language::application::quality_syntax::python_ast::Number::Int(value) if value.to_string() != "0"))));
        let invalid = !matches!(value, ResultValue::Unknown)
            && (!self.protocol_accepts(&value, protocol.expected)
                || (protocol.nonnegative && negative)
                || (protocol.structured && self.invalid_newargs(&value)));
        self.scopes.leave();
        if invalid {
            self.emit(
                protocol.rule,
                protocol.symbol,
                protocol.message,
                self.declaration_range(function.name.range()),
            );
        }
    }
}
