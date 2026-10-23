//! Literal evaluation reports the first rejected node in CPython traversal order.
mod repr;
use super::{errors::Error, value};
use mamba_language::application::quality_syntax::{
    python_ast::{Expr, Operator, UnaryOp},
    PythonTarget, SyntaxDocument,
};
use ruff_text_size::Ranged;

fn number(expression: &Expr) -> Option<&Expr> {
    (!expression.is_number_literal_expr()).then_some(expression)
}

fn signed_number(expression: &Expr) -> Option<&Expr> {
    if let Expr::UnaryOp(unary) = expression {
        if matches!(unary.op, UnaryOp::UAdd | UnaryOp::USub) {
            return number(&unary.operand);
        }
    }
    number(expression)
}

fn first(expression: &Expr) -> Option<&Expr> {
    match expression {
        Expr::StringLiteral(_)
        | Expr::BytesLiteral(_)
        | Expr::NumberLiteral(_)
        | Expr::BooleanLiteral(_)
        | Expr::NoneLiteral(_)
        | Expr::EllipsisLiteral(_) => None,
        Expr::List(value) => value.elts.iter().find_map(first),
        Expr::Tuple(value) => value.elts.iter().find_map(first),
        Expr::Set(value) => value.elts.iter().find_map(first),
        Expr::Dict(value) => {
            for item in &value.items {
                let Some(key) = item.key.as_ref() else {
                    return Some(expression);
                };
                if let Some(invalid) = first(key).or_else(|| first(&item.value)) {
                    return Some(invalid);
                }
            }
            None
        }
        Expr::Call(call)
            if call.arguments.is_empty()
                && matches!(call.func.as_ref(), Expr::Name(name) if name.id == "set") =>
        {
            None
        }
        Expr::BinOp(binary) if matches!(binary.op, Operator::Add | Operator::Sub) => {
            signed_number(&binary.left)
                .or_else(|| number(&binary.right))
                .or_else(|| value::evaluate(expression).err().map(|_| expression))
        }
        _ => signed_number(expression),
    }
}

pub(super) fn error(expression: &Expr, raw: &str, document: &SyntaxDocument<'_>) -> Option<Error> {
    if let Expr::Dict(value) = expression {
        if value.items.iter().any(|item| item.key.is_none()) {
            return Some(Error::parsing(raw, "malformed node or string: None"));
        }
    }
    let node = first(expression)?;
    let line = document.line_column(node.start().to_u32()).0;
    let representation = if document.target == PythonTarget::Py314 {
        repr::expression(node, document.source)
    } else {
        format!("<ast.{} object at {node:p}>", repr::kind(node))
    };
    Some(Error::parsing(
        raw,
        &format!("malformed node or string on line {line}: {representation}"),
    ))
}
