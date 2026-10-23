//! Generic field context checks arguments before the final assignment result.
use super::super::*;
use quality_syntax::python_ast::ExprCall;

impl Checker<'_> {
    pub(in crate::infrastructure::mypy::checker) fn check_dataclass_field_call(
        &mut self,
        call: &ExprCall,
    ) -> bool {
        if !self.scopes.is_dataclass_field(call) {
            return false;
        }
        let Some(expected) = self
            .expected_value
            .and_then(|(range, ty)| (range == call.range()).then_some(ty))
        else {
            return true;
        };
        let contextual = self.scopes.field_context(expected);
        for keyword in &call.arguments.keywords {
            let Some(name) = keyword.arg.as_ref().map(|name| name.as_str()) else {
                continue;
            };
            let (actual, required) = match name {
                "default" => {
                    if !contextual {
                        continue;
                    }
                    let Some(actual) = self.infer_expected(&keyword.value, expected) else {
                        continue;
                    };
                    (actual, expected)
                }
                "default_factory" => {
                    let Some((returns, actual)) = self.scopes.field_factory_type(
                        &keyword.value,
                        &self.environment(),
                        Some(expected),
                    ) else {
                        continue;
                    };
                    let mut signature = signatures::Signature::empty(
                        "field factory".into(),
                        if contextual { expected } else { returns },
                    );
                    signature.name = None;
                    signature.constructor = false;
                    let required = self.context.callable_type(std::sync::Arc::new(signature));
                    (actual, required)
                }
                _ => continue,
            };
            if !self.context.accepts(required, actual) {
                self.emit(
                    "arg-type",
                    format!("Argument \"{name}\" to \"field\" has incompatible type \"{}\"; expected \"{}\"", self.context.name(actual), self.context.name(required)),
                    &keyword.value,
                );
            }
        }
        true
    }
}
