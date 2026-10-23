//! Diagnostic expressions use AST spelling and Python literal representations.
use mamba_language::application::quality_syntax::python_ast::{
    BoolOp, Comprehension, Expr, Number, Operator, UnaryOp,
};
use ruff_text_size::Ranged;
mod interpolations;
mod parameters;

pub(super) fn function_parameters(
    value: &mamba_language::application::quality_syntax::python_ast::Parameters,
    source: &str,
) -> String {
    parameters::display(value, source)
}

fn precedence(value: &Expr) -> u8 {
    match value {
        Expr::Lambda(_) => 0,
        Expr::If(_) => 1,
        Expr::BoolOp(value) => {
            if value.op == BoolOp::Or {
                2
            } else {
                3
            }
        }
        Expr::UnaryOp(value) if value.op == UnaryOp::Not => 4,
        Expr::Named(_) => 5,
        Expr::Compare(_) => 6,
        Expr::BinOp(value) => match value.op {
            Operator::BitOr => 7,
            Operator::BitXor => 8,
            Operator::BitAnd => 9,
            Operator::LShift | Operator::RShift => 10,
            Operator::Add | Operator::Sub => 11,
            Operator::Pow => 14,
            _ => 12,
        },
        Expr::UnaryOp(_) => 13,
        Expr::Await(_) => 15,
        _ => 16,
    }
}

fn child(parent: &Expr, value: &Expr, source: &str, left: bool) -> String {
    let text = expression(value, source);
    let left_associative = !matches!(parent, Expr::If(_))
        && !matches!(parent, Expr::BinOp(value) if value.op == Operator::Pow);
    if precedence(parent) > precedence(value)
        || (precedence(parent) == precedence(value) && left != left_associative)
    {
        format!("({text})")
    } else {
        text
    }
}

fn sequence(values: &[Expr], source: &str) -> String {
    values
        .iter()
        .map(|value| expression(value, source))
        .collect::<Vec<_>>()
        .join(", ")
}

fn generators(values: &[Comprehension], source: &str) -> String {
    values
        .iter()
        .map(|value| {
            let conditions = value
                .ifs
                .iter()
                .map(|value| format!(" if {}", expression(value, source)))
                .collect::<String>();
            format!(
                "{}for {} in {}{conditions}",
                if value.is_async { "async " } else { "" },
                expression(&value.target, source),
                expression(&value.iter, source)
            )
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn float(value: f64) -> String {
    let text = format!("{value:?}");
    if let Some((mantissa, exponent)) = text.split_once('e') {
        let exponent: i32 = exponent.parse().expect("Rust float exponent");
        format!("{mantissa}e{exponent:+03}")
    } else {
        text
    }
}

pub(super) fn expression(value: &Expr, source: &str) -> String {
    match value {
        Expr::Name(name) => name.id.to_string(),
        Expr::StringLiteral(string) => {
            super::string_format::python_string_repr(string.value.to_str())
        }
        Expr::BytesLiteral(bytes) => {
            super::comparison_constants::bytes_repr(&bytes.value.bytes().collect::<Vec<_>>())
        }
        Expr::NumberLiteral(number) => match &number.value {
            Number::Int(value) => value.to_string(),
            Number::Float(value) => float(*value),
            Number::Complex { real: _, imag } => format!("{imag}j"),
        },
        Expr::BooleanLiteral(value) => if value.value { "True" } else { "False" }.into(),
        Expr::NoneLiteral(_) => "None".into(),
        Expr::EllipsisLiteral(_) => "...".into(),
        Expr::Call(call) => {
            let mut arguments: Vec<_> = call
                .arguments
                .args
                .iter()
                .map(|value| expression(value, source))
                .collect();
            arguments.extend(call.arguments.keywords.iter().map(|keyword| {
                format!(
                    "{}{}{}",
                    keyword.arg.as_ref().map_or("**", |name| name.as_str()),
                    if keyword.arg.is_some() { "=" } else { "" },
                    expression(&keyword.value, source)
                )
            }));
            format!(
                "{}({})",
                child(value, &call.func, source, true),
                arguments.join(", ")
            )
        }
        Expr::BinOp(binary) => {
            let operator = if binary.op == Operator::Pow {
                binary.op.to_string()
            } else {
                format!(" {} ", binary.op)
            };
            format!(
                "{}{operator}{}",
                child(value, &binary.left, source, true),
                child(value, &binary.right, source, false)
            )
        }
        Expr::BoolOp(boolean) => boolean
            .values
            .iter()
            .map(|item| child(value, item, source, true))
            .collect::<Vec<_>>()
            .join(&format!(" {} ", boolean.op)),
        Expr::UnaryOp(unary) => format!(
            "{}{}{}",
            unary.op,
            if unary.op == UnaryOp::Not { " " } else { "" },
            child(value, &unary.operand, source, true)
        ),
        Expr::Compare(comparison) => {
            let mut result = child(value, &comparison.operands[0], source, true);
            for (operator, operand) in comparison
                .ops
                .iter()
                .zip(comparison.operands.iter().skip(1))
            {
                result.push_str(&format!(
                    " {operator} {}",
                    child(value, operand, source, false)
                ));
            }
            result
        }
        Expr::If(conditional) => format!(
            "{} if {} else {}",
            child(value, &conditional.body, source, true),
            child(value, &conditional.test, source, true),
            child(value, &conditional.orelse, source, false)
        ),
        Expr::Named(named) => format!(
            "({} := {})",
            expression(&named.target, source),
            expression(&named.value, source)
        ),
        Expr::Starred(starred) => format!("*{}", expression(&starred.value, source)),
        Expr::Lambda(lambda) => {
            let parameters = lambda
                .parameters
                .as_deref()
                .map(|parameters| parameters::display(parameters, source))
                .unwrap_or_default();
            format!(
                "lambda{}{}: {}",
                if parameters.is_empty() { "" } else { " " },
                parameters,
                expression(&lambda.body, source)
            )
        }
        Expr::FString(string) => interpolations::fstring(&string.value, source),
        Expr::Tuple(tuple) => {
            let text = tuple
                .elts
                .iter()
                .map(|value| expression(value, source))
                .collect::<Vec<_>>()
                .join(", ");
            format!("({text}{})", if tuple.elts.len() == 1 { ", " } else { "" })
        }
        Expr::List(list) => format!(
            "[{}]",
            list.elts
                .iter()
                .map(|value| expression(value, source))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Expr::Set(set) => format!("{{{}}}", sequence(&set.elts, source)),
        Expr::Dict(dict) => format!(
            "{{{}}}",
            dict.items
                .iter()
                .map(|item| item.key.as_ref().map_or_else(
                    || format!("**{}", expression(&item.value, source)),
                    |key| format!(
                        "{}: {}",
                        expression(key, source),
                        expression(&item.value, source)
                    )
                ))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Expr::ListComp(comp) => format!(
            "[{} {}]",
            expression(&comp.elt, source),
            generators(&comp.generators, source)
        ),
        Expr::SetComp(comp) => format!(
            "{{{} {}}}",
            expression(&comp.elt, source),
            generators(&comp.generators, source)
        ),
        Expr::Generator(comp) => format!(
            "({} {})",
            expression(&comp.elt, source),
            generators(&comp.generators, source)
        ),
        Expr::DictComp(comp) => {
            let item = comp.key.as_ref().map_or_else(
                || format!("**{}", expression(&comp.value, source)),
                |key| {
                    format!(
                        "{}: {}",
                        expression(key, source),
                        expression(&comp.value, source)
                    )
                },
            );
            format!("{{{item} {}}}", generators(&comp.generators, source))
        }
        Expr::Attribute(attribute) => {
            let mut text = child(value, &attribute.value, source, true);
            if text.chars().all(|ch| ch.is_ascii_digit()) {
                text = format!("({text})");
            }
            format!("{text}.{}", attribute.attr)
        }
        Expr::Subscript(subscript) => {
            let index = expression(&subscript.slice, source);
            let index = if matches!(subscript.slice.as_ref(), Expr::Tuple(tuple) if !tuple.elts.is_empty())
            {
                &index[1..index.len() - 1]
            } else {
                &index
            };
            format!("{}[{index}]", child(value, &subscript.value, source, true))
        }
        Expr::Slice(slice) => {
            let part = |value: &Option<Box<Expr>>| {
                value
                    .as_deref()
                    .map(|value| expression(value, source))
                    .unwrap_or_default()
            };
            let step = part(&slice.step);
            format!(
                "{}:{}{}",
                part(&slice.lower),
                part(&slice.upper),
                if step.is_empty() {
                    step
                } else {
                    format!(":{step}")
                }
            )
        }
        Expr::Await(awaited) => format!("await {}", expression(&awaited.value, source)),
        _ => source[value.range()].to_string(),
    }
}
