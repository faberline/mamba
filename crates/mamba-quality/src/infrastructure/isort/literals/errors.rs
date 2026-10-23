//! The pinned isort exception class and message travel together.
use mamba_language::application::quality_syntax::{
    python_ast::{Expr, Number},
    PythonTarget,
};

pub(super) fn set_element_type_error(kind: &str, target: PythonTarget) -> String {
    unhashable_type_error(kind, "set element", target)
}

pub(super) fn unhashable_type_error(kind: &str, context: &str, target: PythonTarget) -> String {
    if target == PythonTarget::Py314 {
        format!("cannot use '{kind}' as a {context} (unhashable type: '{kind}')")
    } else {
        format!("unhashable type: '{kind}'")
    }
}

pub(super) fn ordering_error(message: String, target: PythonTarget) -> Error {
    let message = message
        .strip_prefix("unhashable type: '")
        .and_then(|m| m.strip_suffix('\''))
        .map_or_else(
            || message.clone(),
            |kind| set_element_type_error(kind, target),
        );
    Error::new("TypeError", message)
}

pub(super) struct Error {
    pub code: &'static str,
    pub message: String,
}

impl Error {
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    pub fn parsing(raw: &str, reason: &str) -> Self {
        Self::new(
            "LiteralParsingFailure",
            format!(
                "isort failed to parse the given literal {raw}. It's important to note that isort literal sorting only supports simple literals parsable by ast.literal_eval which gave the exception of {reason}."
            ),
        )
    }

    pub fn assignments(raw: &str) -> Self {
        Self::new(
            "AssignmentsFormatMismatch",
            format!(
                "isort was told to sort a section of assignments, however the given code:\n\n{raw}\n\nDoes not match isort's strict single line formatting requirement for assignment sorting:\n\n{{variable_name}} = {{value}}\n{{variable_name2}} = {{value2}}\n...\n\n"
            ),
        )
    }

    pub fn mismatch(expression: &Expr, expected: &str) -> Self {
        let actual = expression_type(expression);
        let expected = expected.strip_prefix("unique-").unwrap_or(expected);
        Self::new(
            "LiteralSortTypeMismatch",
            format!(
                "isort was told to sort a literal of type <class '{expected}'> but was given a literal of type <class '{actual}'>."
            ),
        )
    }
}

fn expression_type(expression: &Expr) -> &'static str {
    match expression {
        Expr::List(_) => "list",
        Expr::Tuple(_) => "tuple",
        Expr::Dict(_) => "dict",
        Expr::Set(_) => "set",
        Expr::StringLiteral(_) => "str",
        Expr::BytesLiteral(_) => "bytes",
        Expr::BooleanLiteral(_) => "bool",
        Expr::NoneLiteral(_) => "NoneType",
        Expr::EllipsisLiteral(_) => "ellipsis",
        Expr::NumberLiteral(number) => match number.value {
            Number::Int(_) => "int",
            Number::Float(_) => "float",
            Number::Complex { .. } => "complex",
        },
        Expr::UnaryOp(unary) => expression_type(&unary.operand),
        _ => "object",
    }
}
