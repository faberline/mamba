//! Parameter display retains slash, star, keyword-only, and default spelling.
use mamba_language::application::quality_syntax::python_ast::{ParameterWithDefault, Parameters};

fn parameter(value: &ParameterWithDefault, source: &str) -> String {
    let default = value
        .default
        .as_deref()
        .map(|value| format!("={}", super::expression(value, source)))
        .unwrap_or_default();
    format!("{}{default}", value.parameter.name)
}

pub(super) fn display(value: &Parameters, source: &str) -> String {
    let mut parts: Vec<_> = value
        .posonlyargs
        .iter()
        .map(|value| parameter(value, source))
        .collect();
    if !value.posonlyargs.is_empty() {
        parts.push("/".into());
    }
    parts.extend(value.args.iter().map(|value| parameter(value, source)));
    if let Some(value) = &value.vararg {
        parts.push(format!("*{}", value.name));
    } else if !value.kwonlyargs.is_empty() {
        parts.push("*".into());
    }
    parts.extend(
        value
            .kwonlyargs
            .iter()
            .map(|value| parameter(value, source)),
    );
    if let Some(value) = &value.kwarg {
        parts.push(format!("**{}", value.name));
    }
    parts.join(", ")
}
