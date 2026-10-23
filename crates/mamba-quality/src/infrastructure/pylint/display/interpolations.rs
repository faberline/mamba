//! Interpolation display retains Astroid literal, conversion, and quote choices.
use mamba_language::application::quality_syntax::python_ast::{
    FStringPartRef, FStringValue, InterpolatedStringElement,
};

fn literal(value: &str) -> String {
    let text = crate::infrastructure::pylint::string_format::python_string_repr(value);
    text[1..text.len() - 1]
        .replace('{', "{{")
        .replace('}', "}}")
}

fn elements<'a>(
    values: impl Iterator<Item = &'a InterpolatedStringElement>,
    source: &str,
) -> String {
    values
        .map(|value| match value {
            InterpolatedStringElement::Literal(value) => literal(&value.value),
            InterpolatedStringElement::Interpolation(value) => {
                let mut result = value
                    .debug_text
                    .as_ref()
                    .map(|text| literal(text.as_str()))
                    .unwrap_or_default();
                result.push('{');
                result.push_str(&super::expression(&value.expression, source));
                if let Some(conversion) = value.conversion.to_char().or_else(|| {
                    (value.debug_text.is_some() && value.format_spec.is_none()).then_some('r')
                }) {
                    result.push('!');
                    result.push(conversion);
                }
                if let Some(spec) = &value.format_spec {
                    result.push(':');
                    result.push_str(&elements(spec.elements.iter(), source));
                }
                result.push('}');
                result
            }
        })
        .collect()
}

pub(super) fn fstring(value: &FStringValue, source: &str) -> String {
    let text: String = value
        .iter()
        .map(|part| match part {
            FStringPartRef::Literal(value) => literal(&value.value),
            FStringPartRef::FString(value) => elements(value.elements.iter(), source),
        })
        .collect();
    let quote = ["'", "\"", "\"\"\"", "'''"]
        .into_iter()
        .find(|quote| !text.contains(quote))
        .unwrap_or("'''");
    format!("f{quote}{text}{quote}")
}
