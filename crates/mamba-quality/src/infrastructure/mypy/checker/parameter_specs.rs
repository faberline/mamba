//! Callable prefix errors retain the original hints and argument positions.
use super::*;
use quality_syntax::python_ast::ExprCall;
use signatures::{Kind, Signature};

impl Checker<'_> {
    pub(super) fn check_unmapped_positionals(
        &mut self,
        call: &ExprCall,
        signature: &Signature,
        positional: usize,
    ) {
        let keyword_only = signature
            .parameters
            .iter()
            .filter(|parameter| parameter.kind == Kind::KeywordOnly)
            .count();
        if keyword_only > 0 && call.arguments.args.len() > positional + keyword_only {
            self.emit(
                "call-arg",
                format!("Too many arguments{}", signature.for_function()),
                call,
            );
        }
    }

    pub(super) fn callable_field_hint(
        &self,
        call: &ExprCall,
        signature: &Signature,
    ) -> Option<String> {
        if signature.name.is_some() {
            return None;
        }
        let Expr::Attribute(attribute) = call.func.as_ref() else {
            return None;
        };
        let object = self.infer(&attribute.value)?;
        if self.context.is_class_value(object) {
            return None;
        }
        let field = self.context.class_field(object, attribute.attr.as_str())?;
        (!field.class_var && !field.needs_annotation).then(|| format!("\"{}\" is considered instance variable, to make it class variable use ClassVar[...]", attribute.attr))
    }

    pub(super) fn concatenate_hint(
        &self,
        template: Option<types::Type>,
        expected: types::Type,
        actual: types::Type,
    ) -> Option<String> {
        let template = self.context.callable_signature(template?)?;
        self.context
            .signature_parameter_spec(&template)
            .filter(|&(prefix, _)| prefix > 0)?;
        let expected = self.context.callable_signature(expected)?;
        let actual = self.context.callable_signature(actual)?;
        let names = expected
            .parameters
            .iter()
            .zip(actual.parameters.iter())
            .filter(|(expected, actual)| {
                expected.kind == Kind::PositionalOnly
                    && actual.kind == Kind::PositionalOrKeyword
                    && !actual.name.is_empty()
            })
            .map(|(_, actual)| format!("\"{}\"", actual.name))
            .collect::<Vec<_>>();
        if names.is_empty() {
            return None;
        }
        Some(format!("This is likely because \"{}\" has named arguments: {}. Consider marking them positional-only", actual.name.as_deref().unwrap_or("None"), names.join(", ")))
    }

    pub(super) fn check_extra_positionals(
        &mut self,
        call: &ExprCall,
        signature: &Signature,
        positional: usize,
    ) {
        let keyword_only = signature
            .parameters
            .iter()
            .filter(|parameter| parameter.kind == Kind::KeywordOnly);
        for ((index, value), parameter) in call
            .arguments
            .args
            .iter()
            .enumerate()
            .skip(positional)
            .zip(keyword_only)
        {
            let Some(expected) = parameter.ty else {
                continue;
            };
            if let Some(actual) = self.infer_expected(value, expected) {
                if !self.context.accepts(expected, actual) {
                    let target = signature
                        .display_target()
                        .map(|name| format!(" to {name}"))
                        .unwrap_or_default();
                    self.emit_with_hint(
                        "arg-type",
                        format!(
                            "Argument {}{target} has incompatible type \"{}\"; expected \"{}\"",
                            index + 1,
                            self.context.name(actual),
                            self.context.assignment_type_name(expected)
                        ),
                        value,
                        self.context.variance_hint(expected, actual),
                    );
                }
            }
        }
    }
}
