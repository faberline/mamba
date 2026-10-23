//! Dictionary iteration checks retain original loop, comprehension, and membership contexts.
use super::{resolution::Context, Checker};
use mamba_language::application::quality_syntax::python_ast::{
    visitor::{walk_expr, walk_stmt, Visitor},
    CmpOp, Expr, ExprCall, Operator, Stmt,
};
use ruff_text_size::Ranged;
use std::collections::BTreeSet;

fn keys(expression: &Expr) -> Option<u32> {
    let Expr::Call(call) = expression else {
        return None;
    };
    matches!(call.func.as_ref(), Expr::Attribute(value) if value.attr.as_str() == "keys")
        .then(|| call.start().to_u32())
}
struct Membership<'a> {
    calls: &'a mut BTreeSet<u32>,
    bitwise_parent: bool,
}
impl<'a> Visitor<'a> for Membership<'_> {
    fn visit_expr(&mut self, expression: &'a Expr) {
        if !self.bitwise_parent {
            self.calls.extend(keys(expression));
        }
        let old = self.bitwise_parent;
        self.bitwise_parent = matches!(expression, Expr::BinOp(value) if matches!(value.op, Operator::BitAnd | Operator::BitOr | Operator::BitXor));
        walk_expr(self, expression);
        self.bitwise_parent = old;
    }
}
#[derive(Default)]
struct Iterations(BTreeSet<u32>);
impl<'a> Visitor<'a> for Iterations {
    fn visit_stmt(&mut self, statement: &'a Stmt) {
        if let Stmt::For(value) = statement {
            self.0.extend(keys(&value.iter));
        }
        walk_stmt(self, statement);
    }
    fn visit_expr(&mut self, expression: &'a Expr) {
        let generators: Option<&[_]> = match expression {
            Expr::ListComp(value) => Some(&value.generators),
            Expr::SetComp(value) => Some(&value.generators),
            Expr::DictComp(value) => Some(&value.generators),
            Expr::Generator(value) => Some(&value.generators),
            _ => None,
        };
        if let Some(generators) = generators {
            for generator in generators {
                self.0.extend(keys(&generator.iter));
            }
        }
        if let Expr::Compare(value) = expression {
            for (operator, value) in value.ops.iter().zip(value.operands.iter().skip(1)) {
                if matches!(operator, CmpOp::In | CmpOp::NotIn) {
                    Membership {
                        calls: &mut self.0,
                        bitwise_parent: false,
                    }
                    .visit_expr(value);
                }
            }
        }
        walk_expr(self, expression);
    }
}
pub(super) fn contexts(body: &[Stmt]) -> BTreeSet<u32> {
    let mut contexts = Iterations::default();
    contexts.visit_body(body);
    contexts.0
}
impl Checker<'_> {
    pub(super) fn check_dictionary_iteration(&mut self, call: &ExprCall) {
        if !self.dictionary_iterations.contains(&call.start().to_u32()) {
            return;
        }
        let Expr::Attribute(attribute) = call.func.as_ref() else {
            return;
        };
        if self
            .unpacked_value(
                &attribute.value,
                Context::Scope(self.scopes.current_index()),
            )
            .is_some_and(|value| value.dictionary && value.view.is_none())
        {
            self.emit(
                "C0201",
                "consider-iterating-dictionary",
                "Consider iterating the dictionary directly instead of calling .keys()",
                call.range(),
            );
        }
    }
}
