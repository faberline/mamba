//! Version reachability keeps scalar indexes and tuple slices distinct.
use mamba_language::application::quality_syntax::{
    python_ast::{CmpOp, Expr, ExprCompare, Number},
    PythonTarget,
};

use num_bigint::BigInt;

#[derive(PartialEq, PartialOrd)]
enum Value {
    Scalar(BigInt),
    Tuple(Vec<BigInt>),
}

pub(super) fn truth(compare: &ExprCompare, target: PythonTarget) -> Option<bool> {
    let [left, right] = compare.operands.as_ref() else {
        return None;
    };
    let [op] = compare.ops.as_ref() else {
        return None;
    };
    let (left, op, right) = version(left, target)
        .zip(integer_value(right))
        .map(|(left, right)| (left, *op, right))
        .or_else(|| {
            version(right, target)
                .zip(integer_value(left))
                .map(|(left, right)| (left, reverse(*op), right))
        })?;
    match (&left, &right) {
        (Value::Scalar(_), Value::Scalar(_)) => {}
        (Value::Tuple(left), Value::Tuple(right))
            if left.len() == right.len()
                || (left.len() > right.len() && !matches!(op, CmpOp::Eq | CmpOp::NotEq)) => {}
        _ => return None,
    }
    match op {
        CmpOp::Eq => Some(left == right),
        CmpOp::NotEq => Some(left != right),
        CmpOp::Lt => Some(left < right),
        CmpOp::LtE => Some(left <= right),
        CmpOp::Gt => Some(left > right),
        CmpOp::GtE => Some(left >= right),
        _ => None,
    }
}

fn reverse(op: CmpOp) -> CmpOp {
    match op {
        CmpOp::Lt => CmpOp::Gt,
        CmpOp::LtE => CmpOp::GtE,
        CmpOp::Gt => CmpOp::Lt,
        CmpOp::GtE => CmpOp::LtE,
        op => op,
    }
}

fn sys_version(expression: &Expr) -> bool {
    matches!(expression, Expr::Attribute(attribute) if attribute.attr.as_str() == "version_info" && matches!(attribute.value.as_ref(), Expr::Name(name) if name.id.as_str() == "sys"))
}

fn number(expression: &Expr) -> Option<BigInt> {
    let Expr::NumberLiteral(number) = expression else {
        return None;
    };
    let Number::Int(integer) = &number.value else {
        return None;
    };
    let text = integer.to_string().replace('_', "");
    let (digits, radix) = match text.get(..2) {
        Some("0x" | "0X") => (&text[2..], 16),
        Some("0o" | "0O") => (&text[2..], 8),
        Some("0b" | "0B") => (&text[2..], 2),
        _ => (text.as_str(), 10),
    };
    BigInt::parse_bytes(digits.as_bytes(), radix)
}

fn integer_value(expression: &Expr) -> Option<Value> {
    if let Expr::Tuple(tuple) = expression {
        Some(Value::Tuple(
            tuple.elts.iter().map(number).collect::<Option<_>>()?,
        ))
    } else {
        Some(Value::Scalar(number(expression)?))
    }
}

fn version(expression: &Expr, target: PythonTarget) -> Option<Value> {
    let minor = match target {
        PythonTarget::Py312 => 12,
        PythonTarget::Py313 => 13,
        PythonTarget::Py314 => 14,
    };
    let values = [BigInt::from(3), BigInt::from(minor)];
    if sys_version(expression) {
        return Some(Value::Tuple(values.to_vec()));
    }
    let Expr::Subscript(subscript) = expression else {
        return None;
    };
    if !sys_version(&subscript.value) {
        return None;
    }
    if let Some(index) = number(&subscript.slice) {
        return Some(Value::Scalar(
            values.get(usize::try_from(index).ok()?)?.clone(),
        ));
    }
    let Expr::Slice(slice) = subscript.slice.as_ref() else {
        return None;
    };
    if slice
        .step
        .as_deref()
        .is_some_and(|step| number(step) != Some(BigInt::from(1)))
    {
        return None;
    }
    let low = slice
        .lower
        .as_deref()
        .map(number)
        .unwrap_or(Some(BigInt::from(0)))?;
    let high = slice
        .upper
        .as_deref()
        .map(number)
        .unwrap_or(Some(BigInt::from(2)))?;
    if low < BigInt::from(0) || low >= high || high > BigInt::from(2) {
        return None;
    }
    Some(Value::Tuple(
        values[usize::try_from(low).ok()?..usize::try_from(high).ok()?].to_vec(),
    ))
}
