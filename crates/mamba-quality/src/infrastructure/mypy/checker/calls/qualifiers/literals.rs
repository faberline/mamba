//! Model Final folding uses the pinned supported operations and local Final names.
use super::*;
use quality_syntax::python_ast::{Number, Operator, UnaryOp};
use std::collections::{BTreeMap, BTreeSet};
mod numbers;
use numbers::Numeric;

enum Value {
    Number(Numeric),
    Text(String),
}

pub(in crate::infrastructure::mypy::checker) fn module_constants(
    body: &[Stmt],
    context: &types::Context,
    variables: &types::Variables,
) -> BTreeMap<String, Expr> {
    body.iter()
        .filter_map(|statement| {
            let Stmt::AnnAssign(assignment) = statement else {
                return None;
            };
            let Expr::Name(name) = assignment.target.as_ref() else {
                return None;
            };
            (pydantic::qualifiers::form(&assignment.annotation, context, variables).as_deref()
                == Some("Final"))
            .then(|| Some((name.id.to_string(), assignment.value.as_deref()?.clone())))?
        })
        .collect()
}

pub(super) fn simple(expression: &Expr, constants: &BTreeMap<String, Expr>) -> bool {
    value(expression, constants, &mut BTreeSet::new()).is_some()
}

fn value(
    expression: &Expr,
    constants: &BTreeMap<String, Expr>,
    seen: &mut BTreeSet<String>,
) -> Option<Value> {
    match expression {
        Expr::BooleanLiteral(boolean) => Some(Value::Number(Numeric::Integer(
            u8::from(boolean.value).into(),
        ))),
        Expr::StringLiteral(string) => Some(Value::Text(string.value.to_str().into())),
        Expr::NumberLiteral(number) => Some(Value::Number(match &number.value {
            Number::Int(integer) => Numeric::Integer(numbers::integer(&integer.to_string())?),
            Number::Float(float) => Numeric::Float(*float),
            Number::Complex { .. } => return None,
        })),
        Expr::Name(name) => {
            if !seen.insert(name.id.to_string()) {
                return None;
            }
            let result = constants
                .get(name.id.as_str())
                .and_then(|expression| value(expression, constants, seen));
            seen.remove(name.id.as_str());
            result
        }
        Expr::UnaryOp(unary) => match (unary.op, value(&unary.operand, constants, seen)?) {
            (UnaryOp::UAdd, Value::Number(number)) => Some(Value::Number(number)),
            (UnaryOp::USub, Value::Number(Numeric::Integer(integer))) => {
                Some(Value::Number(Numeric::Integer(-integer)))
            }
            (UnaryOp::USub, Value::Number(Numeric::Float(float))) => {
                Some(Value::Number(Numeric::Float(-float)))
            }
            (UnaryOp::Invert, Value::Number(Numeric::Integer(integer))) => {
                Some(Value::Number(Numeric::Integer(!integer)))
            }
            _ => None,
        },
        Expr::BinOp(binary) => {
            let left = value(&binary.left, constants, seen)?;
            let right = value(&binary.right, constants, seen)?;
            match (binary.op, left, right) {
                (op, Value::Number(left), Value::Number(right)) => {
                    numbers::binary(left, op, right).map(Value::Number)
                }
                (Operator::Add, Value::Text(left), Value::Text(right)) => {
                    Some(Value::Text(left + &right))
                }
                (Operator::Mult, Value::Text(text), Value::Number(Numeric::Integer(count)))
                | (Operator::Mult, Value::Number(Numeric::Integer(count)), Value::Text(text)) => {
                    let count = if count < 0.into() {
                        0
                    } else {
                        count.to_string().parse::<usize>().ok()?
                    };
                    Some(Value::Text(text.repeat(count)))
                }
                _ => None,
            }
        }
        _ => None,
    }
}
