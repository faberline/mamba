//! Conditional inference retains function and generator-specific diagnostics.
use super::{callables::Target, resolution::Context, Checker};
use mamba_language::application::quality_syntax::python_ast::{
    visitor::{walk_stmt, Visitor},
    Expr, Stmt, StmtFunctionDef,
};

#[derive(Default)]
struct GeneratorReturns {
    seen: bool,
    only: bool,
}
impl<'a> Visitor<'a> for GeneratorReturns {
    fn visit_stmt(&mut self, statement: &'a Stmt) {
        match statement {
            Stmt::FunctionDef(_) | Stmt::ClassDef(_) => return,
            Stmt::Return(value) => {
                self.only &= matches!(value.value.as_deref(), Some(Expr::Generator(_)));
                self.seen = true;
            }
            _ => {}
        }
        walk_stmt(self, statement);
    }
}
pub(super) fn only_generator_returns(function: &StmtFunctionDef) -> bool {
    let mut values = GeneratorReturns {
        only: true,
        ..Default::default()
    };
    values.visit_body(&function.body);
    values.seen && values.only
}
impl Checker<'_> {
    pub(super) fn generator_condition(&self, expression: &Expr) -> bool {
        let origin = Context::Scope(self.scopes.current_index());
        let value = if let Expr::Name(name) = expression {
            self.source_binding(name.id.as_str(), &origin)
                .map(|(value, _)| value)
        } else {
            Some(expression.clone())
        };
        let Some(Expr::Call(call)) = value else {
            return false;
        };
        self.expression_target(&call.func).is_some_and(|value|matches!(value.target,Target::Function(function) if function.generator_expression_returns))
    }
    pub(super) fn named_generator_condition(&self, expression: &Expr) -> bool {
        let Expr::Name(name) = expression else {
            return false;
        };
        self.source_binding(
            name.id.as_str(),
            &Context::Scope(self.scopes.current_index()),
        )
        .is_some_and(|(value, _)| matches!(value, Expr::Generator(_)) || matches!(value,Expr::Call(call) if self.expression_target(&call.func).is_some_and(|resolved|matches!(resolved.target,Target::Function(function) if function.generates))))
    }
    pub(super) fn condition_call_missing(&self, expression: &Expr) -> bool {
        if !matches!(expression, Expr::Name(_) | Expr::Attribute(_)) {
            return false;
        }
        self.expression_target(expression).is_some_and(|value|matches!(value.target,Target::Function(function) if matches!(function.category,"function" | "lambda")))
    }
}
