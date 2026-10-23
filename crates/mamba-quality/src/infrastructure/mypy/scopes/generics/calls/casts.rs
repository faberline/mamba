//! A cast returns the type expression supplied by the caller.
use super::*;

impl Scopes {
    pub(in crate::infrastructure::mypy) fn typing_cast_signature(
        &self,
        expression: &Expr,
        environment: &BTreeMap<String, Type>,
    ) -> Option<Arc<Signature>> {
        self.callable_expression_in(expression, environment)
            .filter(|signature| signature.native_symbol == Some("typing.cast"))
    }

    pub(super) fn typing_cast_result(
        &self,
        call: &ExprCall,
        environment: &BTreeMap<String, Type>,
    ) -> Option<Type> {
        self.typing_cast_signature(&call.func, environment)?;
        if call.arguments.args.len() != 2 || !call.arguments.keywords.is_empty() {
            return Some(Type::Any);
        }
        Some(
            call.arguments
                .args
                .first()
                .and_then(|expression| self.annotation(expression))
                .unwrap_or(Type::Any),
        )
    }
}
