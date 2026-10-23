//! Python 3.14 includes deterministic AST fields in literal failure messages.
use mamba_language::application::quality_syntax::python_ast::{
    Expr, ExprContext, Parameter, ParameterWithDefault, Parameters,
};
use ruff_text_size::Ranged;

fn quoted(value: &str) -> String {
    format!("'{}'", value.replace('\\', "\\\\").replace('\'', "\\'"))
}

fn list(values: impl IntoIterator<Item = String>) -> String {
    let values: Vec<_> = values.into_iter().collect();
    if values.len() > 2 {
        format!("[{}, ..., {}]", values[0], values.last().unwrap())
    } else {
        format!("[{}]", values.join(", "))
    }
}

fn context(value: ExprContext, depth: u8) -> &'static str {
    if depth == 0 {
        return match value {
            ExprContext::Store => "Store(...)",
            ExprContext::Del => "Del(...)",
            _ => "Load(...)",
        };
    }
    match value {
        ExprContext::Load | ExprContext::Invalid => "Load()",
        ExprContext::Store => "Store()",
        ExprContext::Del => "Del()",
    }
}

fn optional(value: Option<&Expr>, source: &str, depth: u8) -> String {
    value.map_or_else(
        || "None".into(),
        |value| expression_depth(value, source, depth),
    )
}

fn parameter(value: &Parameter, source: &str, depth: u8) -> String {
    if depth == 0 {
        return "arg(...)".into();
    }
    format!(
        "arg(arg={}, annotation={}, type_comment=None)",
        quoted(value.name.as_str()),
        optional(value.annotation.as_deref(), source, depth - 1)
    )
}

fn parameters(value: Option<&Parameters>, source: &str, depth: u8) -> String {
    if depth == 0 {
        return "arguments(...)".into();
    }
    let args = |values: &[ParameterWithDefault]| {
        list(
            values
                .iter()
                .map(|value| parameter(&value.parameter, source, depth - 1)),
        )
    };
    let variadic = |value: Option<&Parameter>| {
        value.map_or_else(
            || "None".into(),
            |value| parameter(value, source, depth - 1),
        )
    };
    let Some(value) = value else {
        return "arguments(posonlyargs=[], args=[], vararg=None, kwonlyargs=[], kw_defaults=[], kwarg=None, defaults=[])".into();
    };
    format!(
        "arguments(posonlyargs={}, args={}, vararg={}, kwonlyargs={}, kw_defaults={}, kwarg={}, defaults={})",
        args(&value.posonlyargs),
        args(&value.args),
        variadic(value.vararg.as_deref()),
        args(&value.kwonlyargs),
        list(value.kwonlyargs.iter().map(|value| optional(
            value.default.as_deref(),
            source,
            depth - 1
        ))),
        variadic(value.kwarg.as_deref()),
        list(
            value
                .posonlyargs
                .iter()
                .chain(&value.args)
                .filter_map(|value| value.default.as_deref())
                .map(|value| expression_depth(value, source, depth - 1))
        ),
    )
}

pub(super) fn kind(value: &Expr) -> &'static str {
    match value {
        Expr::Name(_) => "Name",
        Expr::Lambda(_) => "Lambda",
        Expr::Starred(_) => "Starred",
        Expr::UnaryOp(_) => "UnaryOp",
        Expr::BinOp(_) => "BinOp",
        Expr::Call(_) => "Call",
        Expr::Attribute(_) => "Attribute",
        Expr::Subscript(_) => "Subscript",
        Expr::Dict(_) => "Dict",
        Expr::List(_) => "List",
        Expr::Tuple(_) => "Tuple",
        Expr::Set(_) => "Set",
        _ => "Constant",
    }
}

pub(super) fn expression(value: &Expr, source: &str) -> String {
    expression_depth(value, source, 3)
}

fn expression_depth(value: &Expr, source: &str, depth: u8) -> String {
    if depth == 0 {
        return format!("{}(...)", kind(value));
    }
    match value {
        Expr::Name(value) => format!(
            "Name(id={}, ctx={})",
            quoted(value.id.as_str()),
            context(value.ctx, depth - 1)
        ),
        Expr::Lambda(value) => format!(
            "Lambda(args={}, body={})",
            parameters(value.parameters.as_deref(), source, depth - 1),
            expression_depth(&value.body, source, depth - 1)
        ),
        Expr::Starred(value) => format!(
            "Starred(value={}, ctx={})",
            expression_depth(&value.value, source, depth - 1),
            context(value.ctx, depth - 1)
        ),
        Expr::UnaryOp(value) => format!(
            "UnaryOp(op={:?}(), operand={})",
            value.op,
            expression_depth(&value.operand, source, depth - 1)
        ),
        Expr::BinOp(value) => format!(
            "BinOp(left={}, op={:?}(), right={})",
            expression_depth(&value.left, source, depth - 1),
            value.op,
            expression_depth(&value.right, source, depth - 1)
        ),
        Expr::NumberLiteral(_)
        | Expr::BooleanLiteral(_)
        | Expr::NoneLiteral(_)
        | Expr::StringLiteral(_)
        | Expr::BytesLiteral(_) => {
            let representation = super::super::value::evaluate(value)
                .map_or_else(|_| source[value.range()].to_owned(), |value| value.repr);
            format!("Constant(value={representation}, kind=None)")
        }
        Expr::EllipsisLiteral(_) => "Constant(value=Ellipsis, kind=None)".into(),
        Expr::List(value) => format!(
            "List(elts={}, ctx={})",
            list(
                value
                    .elts
                    .iter()
                    .map(|value| expression_depth(value, source, depth - 1))
            ),
            context(value.ctx, depth - 1)
        ),
        Expr::Tuple(value) => format!(
            "Tuple(elts={}, ctx={})",
            list(
                value
                    .elts
                    .iter()
                    .map(|value| expression_depth(value, source, depth - 1))
            ),
            context(value.ctx, depth - 1)
        ),
        Expr::Set(value) => format!(
            "Set(elts={})",
            list(
                value
                    .elts
                    .iter()
                    .map(|value| expression_depth(value, source, depth - 1))
            )
        ),
        Expr::Attribute(value) => format!(
            "Attribute(value={}, attr={}, ctx={})",
            expression_depth(&value.value, source, depth - 1),
            quoted(value.attr.as_str()),
            context(value.ctx, depth - 1)
        ),
        Expr::Subscript(value) => format!(
            "Subscript(value={}, slice={}, ctx={})",
            expression_depth(&value.value, source, depth - 1),
            expression_depth(&value.slice, source, depth - 1),
            context(value.ctx, depth - 1)
        ),
        _ => format!("{}()", kind(value)),
    }
}
