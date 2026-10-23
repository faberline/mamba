//! Conditional and assertion diagnostics retain their separate inference policies.
use super::{callables::Target, Checker};
use mamba_language::application::quality_syntax::python_ast::{Expr, StmtAssert};
use ruff_text_size::Ranged;

impl Checker<'_> {
    pub(super) fn check_constant_condition(
        &mut self,
        expression: &Expr,
        parent: ruff_text_size::TextRange,
    ) {
        if self.generator_condition(expression) {
            self.emit(
                "W0125",
                "using-constant-test",
                "Using a conditional statement with a constant value",
                parent,
            );
            return;
        }
        let constant = match expression {
            Expr::StringLiteral(_)
            | Expr::BytesLiteral(_)
            | Expr::NumberLiteral(_)
            | Expr::BooleanLiteral(_)
            | Expr::NoneLiteral(_)
            | Expr::EllipsisLiteral(_)
            | Expr::List(_)
            | Expr::Tuple(_)
            | Expr::Set(_)
            | Expr::Dict(_)
            | Expr::Generator(_)
            | Expr::Lambda(_) => true,
            Expr::Name(_) | Expr::Attribute(_) => {
                matches!(
                    self.callable_target(expression),
                    Some(Target::Function(_) | Target::Class(_) | Target::Module(_))
                ) || matches!(expression,Expr::Name(name) if self.modules.function(name.id.as_str()) && !self.scopes.declared(name.id.as_str()))
                    || self.named_generator_condition(expression)
            }
            _ => false,
        };
        if constant {
            if self.condition_call_missing(expression) {
                self.emit("W0126", "missing-parentheses-for-call-in-test", "Using a conditional statement with potentially wrong function or method call due to missing parentheses", expression.range());
            }
            self.emit(
                "W0125",
                "using-constant-test",
                "Using a conditional statement with a constant value",
                expression.range(),
            );
        }
    }

    pub(super) fn check_assertion(&mut self, assertion: &StmtAssert) {
        match assertion.test.as_ref() {
            Expr::Tuple(tuple) if !tuple.elts.is_empty() => self.emit(
                "W0199",
                "assert-on-tuple",
                "Assert called on a populated tuple. Did you mean 'assert x,y'?",
                assertion.range(),
            ),
            Expr::StringLiteral(string) => {
                let when = if string.value.to_str().is_empty() {
                    "always"
                } else {
                    "never"
                };
                self.emit("W0129", "assert-on-string-literal", &format!("Assert statement has a string literal as its first argument. The assert will {when} fail."), assertion.range());
            }
            _ => {}
        }
    }
}
