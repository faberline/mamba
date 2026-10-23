//! Python AST target-context names used in original error messages.
use ruff_python_ast::Expr;

pub(super) fn is_context_error(message: &str) -> bool {
    matches!(
        message,
        "Invalid assignment target"
            | "Invalid augmented assignment target"
            | "Invalid annotated assignment target"
            | "Invalid delete target"
            | "Assignment expression target must be an identifier"
            | "Expected name after `as`"
    )
}

pub(super) fn name(expr: &Expr) -> &'static str {
    match expr {
        Expr::Name(_) => "name",
        Expr::Attribute(_) => "attribute",
        Expr::Subscript(_) => "subscript",
        Expr::Starred(_) => "starred",
        Expr::Named(_) => "named expression",
        Expr::Call(_) => "function call",
        Expr::Tuple(_) => "tuple",
        Expr::List(_) => "list",
        Expr::Lambda(_) => "lambda",
        Expr::BooleanLiteral(value) => {
            if value.value {
                "True"
            } else {
                "False"
            }
        }
        Expr::NoneLiteral(_) => "None",
        Expr::NumberLiteral(_) | Expr::StringLiteral(_) | Expr::BytesLiteral(_) => "literal",
        _ => "expression",
    }
}

pub(super) fn invalid(expr: &Expr) -> Option<&Expr> {
    match expr {
        Expr::Name(_) | Expr::Attribute(_) | Expr::Subscript(_) => None,
        Expr::Tuple(value) => value.elts.iter().find_map(invalid),
        Expr::List(value) => value.elts.iter().find_map(invalid),
        Expr::Starred(value) => invalid(&value.value),
        _ => Some(expr),
    }
}

pub(super) fn assignment(expr: &Expr, suggestion: bool) -> Option<String> {
    let expr = invalid(expr)?;
    let name = name(expr);
    let suffix = if suggestion && !matches!(name, "True" | "False" | "None" | "named expression") {
        " here. Maybe you meant '==' instead of '='?"
    } else {
        ""
    };
    Some(format!("cannot assign to {name}{suffix}"))
}
