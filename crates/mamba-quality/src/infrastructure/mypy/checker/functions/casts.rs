//! Cast checks use the imported callable identity and normal annotation rules.
use super::*;
use quality_syntax::python_ast::ExprCall;
use ruff_text_size::{TextRange, TextSize};

impl Checker<'_> {
    pub(super) fn check_typing_cast(&mut self, call: &ExprCall) -> bool {
        let Some(signature) = self
            .scopes
            .typing_cast_signature(&call.func, &self.environment())
        else {
            return false;
        };
        if !call.arguments.keywords.is_empty() {
            self.emit(
                "misc",
                "\"cast\" must be called with 2 positional arguments".into(),
                call,
            );
            return true;
        }
        if call.arguments.args.len() != 2 {
            self.emit("misc", "\"cast\" expects 2 arguments".into(), call);
            let count = call.arguments.args.len() + call.arguments.keywords.len();
            let arguments = std::iter::repeat_n("\"Any\"", count).collect::<Vec<_>>();
            let message = if count == 0 {
                format!(
                    "All overload variants of \"{}\" require at least one argument",
                    signature.name.as_deref().unwrap_or("cast")
                )
            } else {
                format!(
                    "No overload variant of \"{}\" matches argument {} {}",
                    signature.name.as_deref().unwrap_or("cast"),
                    if count == 1 { "type" } else { "types" },
                    arguments.join(", "),
                )
            };
            let variants = signature
                .overloads
                .iter()
                .map(|variant| format!("    {}", self.context.pretty_callable(variant)))
                .collect::<Vec<_>>()
                .join("\n");
            self.emit_with_hint(
                "call-overload",
                message,
                call,
                Some(format!("Possible overload variants:\n{variants}")),
            );
            return true;
        }
        let target = &call.arguments.args[0];
        let value = &call.arguments.args[1];
        let Some(expected) = self.scopes.annotation(target) else {
            self.check_callable_annotation(target);
            return true;
        };
        if !expected.is_any()
            && (self.infer(value) == Some(expected)
                || self.context.literal_expression(value) == Some(expected))
        {
            let message = format!("Redundant cast to \"{}\"", self.context.name(expected));
            let location = TextRange::at(call.start(), TextSize::new(1));
            if !self.loop_redundant_cast(
                expected,
                message.clone(),
                location,
                call.range(),
                self.context.source_declaration_site(call),
            ) {
                self.emit("redundant-cast", message, &location);
            }
        }
        true
    }
}
