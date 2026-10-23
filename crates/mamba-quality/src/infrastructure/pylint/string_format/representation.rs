//! Diagnostic strings use Python's literal representation.
pub(super) fn repr(text: &str) -> String {
    let quote = if text.contains('\'') && !text.contains('"') {
        '"'
    } else {
        '\''
    };
    let mut result = quote.to_string();
    for c in text.chars() {
        match c {
            '\\' => result.push_str("\\\\"),
            '\n' => result.push_str("\\n"),
            '\r' => result.push_str("\\r"),
            '\t' => result.push_str("\\t"),
            c if c == quote => {
                result.push('\\');
                result.push(c);
            }
            c if c.is_control() && u32::from(c) < 256 => {
                result.push_str(&format!("\\x{:02x}", u32::from(c)))
            }
            c => result.push(c),
        }
    }
    result.push(quote);
    result
}

pub(super) fn key(key: &super::brace::Key) -> String {
    match key {
        super::brace::Key::Index(value) => value.to_string(),
        super::brace::Key::Name(value) => repr(value),
    }
}

pub(super) fn constant(
    expression: &mamba_language::application::quality_syntax::python_ast::Expr,
) -> String {
    use mamba_language::application::quality_syntax::python_ast::{Expr, Number};
    match expression {
        Expr::NoneLiteral(_) => "%s".into(),
        Expr::BooleanLiteral(value) => if value.value { "True" } else { "False" }.into(),
        Expr::NumberLiteral(value) => match &value.value {
            Number::Int(value) => value.to_string(),
            Number::Float(value) => format!("{value:?}"),
            Number::Complex { real: _, imag } => format!("{imag}j"),
        },
        Expr::BytesLiteral(value) => format!(
            "b{}",
            repr(&value.value.bytes().map(char::from).collect::<String>())
        ),
        Expr::StringLiteral(value) => value.value.to_str().to_string(),
        _ => "%s".into(),
    }
}
