//! Await expressions and unused coroutine statements retain original diagnostics.
use super::*;
use quality_syntax::python_ast::{ExprAwait, StmtExpr};
use types::Type;

impl Checker<'_> {
    pub(super) fn check_await(&mut self, expression: &ExprAwait) {
        let Some(actual) = self.infer(&expression.value) else {
            return;
        };
        match self.context.await_result(actual) {
            None => {
                let hint = self
                    .context
                    .variance_hint(self.context.awaitable_protocol(), actual);
                self.emit_with_hint(
                    "misc",
                    format!(
                        "Incompatible types in \"await\" (actual type \"{}\", expected type \"Awaitable[Any]\")",
                        self.context.name(actual)
                    ),
                    expression,
                    hint,
                );
            }
            Some(Type::None) if self.none_allowed != Some(expression.range()) => self.emit(
                "func-returns-value",
                "Function does not return a value (it only ever returns None)".into(),
                expression,
            ),
            _ => {}
        }
    }

    pub(in crate::infrastructure::mypy::checker) fn visit_expression_statement<'a>(
        &mut self,
        statement: &'a StmtExpr,
    ) where
        Self: Visitor<'a>,
    {
        let previous = self.none_allowed;
        self.none_allowed = Some(statement.value.range());
        self.visit_expr(&statement.value);
        self.none_allowed = previous;
        if let Some(ty) = self.infer(&statement.value) {
            if let Some(coroutine) = self
                .context
                .canonical_coroutine_diagnostic_view(ty, self.python_target)
            {
                self.emit_with_hint(
                    "unused-coroutine",
                    format!(
                        "Value of type \"{}\" must be used",
                        self.context.name(coroutine)
                    ),
                    statement,
                    Some("Are you missing an await?".into()),
                );
            }
            if ty == Type::Never {
                self.flow.reachable = false;
            }
        }
    }

    pub(in crate::infrastructure::mypy::checker) fn visit_reveal_value<'a>(
        &mut self,
        expression: &'a Expr,
    ) -> bool
    where
        Self: Visitor<'a>,
    {
        let Expr::Call(call) = expression else {
            return false;
        };
        if !matches!(call.func.as_ref(), Expr::Name(name) if name.id.as_str() == "reveal_type") {
            return false;
        }
        let [value] = call.arguments.args.as_ref() else {
            return false;
        };
        self.check_alias_reveal(call);
        self.visit_expr(&call.func);
        let previous = self.none_allowed;
        self.none_allowed = Some(value.range());
        self.visit_expr(value);
        self.none_allowed = previous;
        true
    }

    pub(in crate::infrastructure::mypy::checker) fn check_async_annotation(
        &mut self,
        expression: &Expr,
    ) -> bool {
        let (base, arguments) = match expression {
            Expr::Subscript(subscript) => {
                let arguments: Vec<_> = match subscript.slice.as_ref() {
                    Expr::Tuple(tuple) => tuple.elts.iter().collect(),
                    value => vec![value],
                };
                (subscript.value.as_ref(), Some(arguments))
            }
            Expr::Name(_) | Expr::Attribute(_) => (expression, None),
            _ => return false,
        };
        let variables = self.scopes.variables();
        let name = self.context.typing_form(base, &variables).or_else(|| {
            let binding = self.infer(base)?;
            if !self.context.is_class_value(binding) {
                return None;
            }
            self.context
                .canonical_coroutine_diagnostic_view(
                    self.context.annotation_value(binding),
                    self.python_target,
                )
                .map(|_| "Coroutine".to_owned())
        });
        let Some(name) = name else {
            return false;
        };
        let Some(kind) = types::Context::asynchronous_kind(&name) else {
            return false;
        };
        let (minimum, maximum) = types::Context::asynchronous_arity(kind);
        if let Some(arguments) = arguments {
            let count = arguments.len();
            if !(minimum..=maximum).contains(&count) {
                let expected = if minimum == maximum {
                    minimum.to_string()
                } else {
                    format!("between {minimum} and {maximum}")
                };
                self.emit(
                    "type-arg",
                    format!(
                        "\"{name}\" expects {expected} type argument{}, but {count} given",
                        if maximum == 1 { "" } else { "s" }
                    ),
                    expression,
                );
                if count < minimum {
                    self.emit(
                        "type-arg",
                        format!("Missing type arguments for generic type \"{name}\""),
                        expression,
                    );
                }
            }
            for argument in arguments {
                self.check_callable_annotation(argument);
            }
        } else {
            self.emit(
                "type-arg",
                format!("Missing type arguments for generic type \"{name}\""),
                expression,
            );
        }
        true
    }
}
