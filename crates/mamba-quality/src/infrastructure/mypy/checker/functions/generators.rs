//! Generator diagnostics use declared yield, send, and final return components.
use super::*;
use ruff_text_size::{TextRange, TextSize};
use types::Type;

impl Checker<'_> {
    pub(in crate::infrastructure::mypy::checker) fn visit_generator_lambda<'a>(
        &mut self,
        expression: &'a Expr,
    ) -> bool
    where
        Self: Visitor<'a>,
    {
        let Expr::Lambda(lambda) = expression else {
            return false;
        };
        let Some(signature) = Signature::generator_lambda(lambda) else {
            return false;
        };
        if let Some(parameters) = &lambda.parameters {
            for parameter in parameters.iter_non_variadic_params() {
                if let Some(default) = parameter.default.as_deref() {
                    self.visit_expr(default);
                }
            }
        }
        let environment = signature
            .parameters
            .iter()
            .map(|parameter| (parameter.name.clone(), parameter.ty))
            .collect();
        self.scopes.enter(
            scopes::Kind::Function,
            &[],
            environment,
            self.scopes.variables(),
        );
        self.scopes.set_generator(Some((Type::Any, false)), &[]);
        let previous_flow = std::mem::take(&mut self.flow);
        let previous_expected = self.expected_value.take();
        self.visit_expr(&lambda.body);
        self.flow = previous_flow;
        self.expected_value = previous_expected;
        self.scopes.leave();
        true
    }

    pub(in crate::infrastructure::mypy::checker) fn check_generator_declaration(
        &mut self,
        function: &StmtFunctionDef,
        returns: Option<Type>,
    ) {
        if !Signature::is_generator(function)
            || self
                .context
                .generator_return_valid(returns.unwrap_or(Type::Any), function.is_async)
        {
            return;
        }
        let (kind, container) = if function.is_async {
            ("an async generator", "AsyncGenerator")
        } else {
            ("a generator", "Generator")
        };
        let point = TextRange::new(function.start(), function.start() + TextSize::new(1));
        self.emit(
            "misc",
            format!(
                "The return type of {kind} function should be \"{container}\" or one of its supertypes"
            ),
            &point,
        );
    }

    pub(super) fn check_generator_expression(&mut self, expression: &Expr) -> bool {
        let Some((declared, _)) = self.scopes.generator() else {
            return false;
        };
        let expected = self.context.generator_parts(declared).yielding;
        match expression {
            Expr::Yield(yielded) => {
                if let Some(value) = yielded.value.as_deref() {
                    if let Some(actual) = self.infer_expected(value, expected) {
                        self.check_yield_type("yield", expected, actual, expression);
                    }
                } else if !matches!(expected, Type::None | Type::Any | Type::AnySpecial) {
                    self.emit("misc", "Yield value expected".into(), expression);
                }
            }
            Expr::YieldFrom(delegated) => {
                let Some(actual) = self.infer(&delegated.value) else {
                    return true;
                };
                if let Some(parts) = self.context.delegated_generator(actual) {
                    self.check_yield_type("yield from", expected, parts.yielding, expression);
                    if parts.returning == Type::None
                        && self.none_allowed != Some(expression.range())
                    {
                        self.emit(
                            "func-returns-value",
                            "Function does not return a value (it only ever returns None)".into(),
                            expression,
                        );
                    }
                } else {
                    self.emit(
                        "misc",
                        format!(
                            "\"yield from\" can't be applied to \"{}\"",
                            self.context.name(actual)
                        ),
                        expression,
                    );
                }
            }
            _ => return false,
        }
        true
    }

    fn check_yield_type(
        &mut self,
        operation: &str,
        expected: Type,
        actual: Type,
        expression: &Expr,
    ) {
        if !self.context.accepts(expected, actual) {
            let hint = self.context.variance_hint(expected, actual);
            self.emit_with_hint(
                "misc",
                format!(
                    "Incompatible types in \"{operation}\" (actual type \"{}\", expected type \"{}\")",
                    self.context.name(actual),
                    self.context.name(expected)
                ),
                expression,
                hint,
            );
        }
    }
}
