//! Name-free truth inference keeps literal values without evaluating project code.
use super::{simplifiable_numbers as numeric, values::Value, Checker};
use mamba_language::application::quality_syntax::python_ast::{
    BoolOp, CmpOp, Expr, FStringPartRef, InterpolatedStringElement, Operator, UnaryOp,
};

fn literal_truth(value: Value) -> Option<bool> {
    match value {
        Value::Boolean(value) => Some(value),
        Value::String(value) => Some(!value.is_empty()),
        Value::Bytes(value) => Some(!value.is_empty()),
        Value::Sequence(value, _) => Some(!value.is_empty()),
        Value::Mapping(value) => Some(!value.is_empty()),
        Value::Scalar("NoneType") => Some(false),
        _ => None,
    }
}
fn compare(left: &Expr, op: CmpOp, right: &Expr) -> Option<bool> {
    let order = numeric::number(left)?.compare(&numeric::number(right)?)?;
    Some(match op {
        CmpOp::Eq => order.is_eq(),
        CmpOp::NotEq => !order.is_eq(),
        CmpOp::Lt => order.is_lt(),
        CmpOp::LtE => !order.is_gt(),
        CmpOp::Gt => order.is_gt(),
        CmpOp::GtE => !order.is_lt(),
        _ => return None,
    })
}
impl Checker<'_> {
    pub(super) fn simplification_truth(&self, value: &Expr) -> Option<bool> {
        if let Some(number) = numeric::number(value) {
            return Some(number.truth());
        }
        if let Some(truth) = literal_truth(Value::from_expression(value)) {
            return Some(truth);
        }
        match value {
            Expr::EllipsisLiteral(_) | Expr::Lambda(_) => Some(true),
            Expr::UnaryOp(value) if value.op == UnaryOp::Not => self
                .simplification_truth(&value.operand)
                .map(|value| !value),
            Expr::BoolOp(value) => {
                for child in &value.values {
                    let truth = self.simplification_truth(child)?;
                    if truth == (value.op == BoolOp::Or) {
                        return Some(truth);
                    }
                }
                Some(value.op == BoolOp::And)
            }
            Expr::Compare(value) => {
                for (pair, op) in value.operands.windows(2).zip(&value.ops) {
                    if !compare(&pair[0], *op, &pair[1])? {
                        return Some(false);
                    }
                }
                Some(true)
            }
            Expr::BinOp(value) => {
                let (left, right) = (value.left.as_ref(), value.right.as_ref());
                if value.op == Operator::Add
                    && matches!(
                        (left, right),
                        (Expr::StringLiteral(_), Expr::StringLiteral(_))
                    )
                {
                    return Some(
                        self.simplification_truth(left)? || self.simplification_truth(right)?,
                    );
                }
                if value.op == Operator::Mult {
                    let (sequence, count) = if matches!(
                        left,
                        Expr::StringLiteral(_)
                            | Expr::BytesLiteral(_)
                            | Expr::List(_)
                            | Expr::Tuple(_)
                    ) {
                        (left, numeric::number(right)?)
                    } else if matches!(
                        right,
                        Expr::StringLiteral(_)
                            | Expr::BytesLiteral(_)
                            | Expr::List(_)
                            | Expr::Tuple(_)
                    ) {
                        (right, numeric::number(left)?)
                    } else {
                        return None;
                    };
                    let numeric::Numeric::Integer(count) = count else {
                        return None;
                    };
                    return Some(
                        count > num_bigint::BigInt::from(0)
                            && self.simplification_truth(sequence)?,
                    );
                }
                None
            }
            Expr::Subscript(value) => {
                let elements = match value.value.as_ref() {
                    Expr::List(value) => &value.elts,
                    Expr::Tuple(value) => &value.elts,
                    _ => return None,
                };
                let numeric::Numeric::Integer(index) = numeric::number(&value.slice)? else {
                    return None;
                };
                let index: i64 = index.to_string().parse().ok()?;
                let index = if index < 0 {
                    i64::try_from(elements.len()).ok()?.checked_add(index)?
                } else {
                    index
                };
                self.simplification_truth(elements.get(usize::try_from(index).ok()?)?)
            }
            Expr::FString(value) => {
                let mut truth = false;
                for part in value.value.iter() {
                    match part {
                        FStringPartRef::Literal(value) => truth |= !value.value.is_empty(),
                        FStringPartRef::FString(value) => {
                            for element in &value.elements {
                                match element {
                                    InterpolatedStringElement::Literal(value) => {
                                        truth |= !value.value.is_empty()
                                    }
                                    InterpolatedStringElement::Interpolation(value) => {
                                        if value.format_spec.is_some() {
                                            return None;
                                        }
                                        match value.expression.as_ref() {
                                            Expr::StringLiteral(literal) => {
                                                truth |= !literal.value.to_str().is_empty()
                                                    || value.conversion.to_char() == Some('r')
                                            }
                                            Expr::NumberLiteral(_)
                                            | Expr::BooleanLiteral(_)
                                            | Expr::NoneLiteral(_) => truth = true,
                                            _ => return None,
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                Some(truth)
            }
            _ => None,
        }
    }
}
