//! Lambda values receive the caller's parameter types before their body is inferred.
use super::*;
use types::Type;

impl Checker<'_> {
    pub(super) fn infer_lambda_expected(
        &mut self,
        expression: &Expr,
        expected: Type,
    ) -> Option<Option<Type>> {
        let Expr::Lambda(lambda) = expression else {
            return None;
        };
        let contextual = self.scopes.lambda_expected_context(lambda, expected);
        let target = self.context.callable_signature(contextual)?;
        if self
            .scopes
            .lambda_context_mismatch(lambda, contextual, &target)
        {
            self.emit("misc", "Cannot infer type of lambda".into(), expression);
            // No context can be used here. Preserve unknown recovery without replay.
            return Some(self.scopes.lambda_type(lambda, &self.environment()));
        }
        let actual = self
            .scopes
            .lambda_type_expected(lambda, &self.environment(), contextual);
        let Some(actual) = actual else {
            // Only newly selected Union context owns this unknown result.
            return (contextual != expected).then_some(None);
        };
        let Some(signature) = self.context.callable_signature(actual) else {
            return (contextual != expected).then_some(None);
        };
        if signature.parameters.len() != target.parameters.len() {
            self.emit("misc", "Cannot infer type of lambda".into(), expression);
            return Some(Some(actual));
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
        let previous_flow = std::mem::take(&mut self.flow);
        let returned = target.returns.unwrap_or(Type::Any);
        if let Some(actual) = self.infer_expected(&lambda.body, returned) {
            if !self.context.accepts(returned, actual) {
                self.emit(
                    "return-value",
                    format!(
                        "Incompatible return value type (got \"{}\", expected \"{}\")",
                        self.context.name(actual),
                        self.context.name(returned)
                    ),
                    lambda.body.as_ref(),
                );
            }
        }
        self.flow = previous_flow;
        self.scopes.leave();
        Some(Some(actual))
    }
}
