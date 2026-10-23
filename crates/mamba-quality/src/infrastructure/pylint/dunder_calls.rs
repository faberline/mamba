//! Dunder calls use source ancestry and the original inferred-node category.
use super::{
    callables::Target, dunder_data, operand_inference::Operand, resolution::Context, Checker,
};
use mamba_language::application::quality_syntax::python_ast::{
    visitor::{walk_expr, walk_stmt, Visitor},
    Expr, ExprCall, Stmt,
};
use ruff_text_size::Ranged;
use std::collections::BTreeSet;

#[derive(Default)]
pub(super) struct Exemptions(BTreeSet<u32>);
#[derive(Default)]
struct Collector {
    exempt: Exemptions,
    dunder: bool,
    lambda: bool,
}
impl Exemptions {
    pub(super) fn new(body: &[Stmt]) -> Self {
        let mut collector = Collector::default();
        collector.visit_body(body);
        collector.exempt
    }
}
impl<'a> Visitor<'a> for Collector {
    fn visit_stmt(&mut self, statement: &'a Stmt) {
        let previous = self.dunder;
        if let Stmt::FunctionDef(function) = statement {
            let name = function.name.as_str();
            self.dunder |= name.starts_with("__") && name.ends_with("__");
        }
        walk_stmt(self, statement);
        self.dunder = previous;
    }
    fn visit_expr(&mut self, expression: &'a Expr) {
        let previous = self.lambda;
        self.lambda |= matches!(expression, Expr::Lambda(_));
        if let Expr::Call(call) = expression {
            if let Expr::Attribute(attribute) = call.func.as_ref() {
                if self.dunder
                    || (self.lambda && dunder_data::lambda_exception(attribute.attr.as_str()))
                {
                    self.exempt.0.insert(call.start().to_u32());
                }
            }
        }
        walk_expr(self, expression);
        self.lambda = previous;
    }
}
impl Checker<'_> {
    pub(super) fn check_dunder_call(&mut self, call: &ExprCall) {
        let Expr::Attribute(attribute) = call.func.as_ref() else {
            return;
        };
        let name = attribute.attr.as_str();
        let Some(suggestion) = dunder_data::suggestion(name) else {
            return;
        };
        if self.dunder_exemptions.0.contains(&call.start().to_u32())
            || matches!(attribute.value.as_ref(), Expr::Call(receiver) if matches!(receiver.func.as_ref(), Expr::Name(name) if name.id.as_str() == "super"))
        {
            return;
        }
        let origin = Context::Scope(self.scopes.current_index());
        let alternatives = self.operand_nodes(&attribute.value, origin.clone());
        if alternatives.len() == 1
            && (matches!(
                self.operand_in(&attribute.value, origin, &mut Default::default()),
                Operand::Class(_, _) | Operand::Function(_) | Operand::Generator
            ) || self
                .expression_target(&attribute.value)
                .is_some_and(|value| matches!(value.target, Target::Module(_))))
        {
            return;
        }
        self.emit(
            "C2801",
            "unnecessary-dunder-call",
            &format!("Unnecessarily calls dunder method {name}. {suggestion}."),
            call.range(),
        );
    }
}
