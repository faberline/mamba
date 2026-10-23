//! Constant comparisons retain Python equality and value spelling.
use mamba_language::application::quality_syntax::python_ast::{Expr, Number};
use num_bigint::BigInt;
use num_rational::BigRational;

enum Scalar {
    Finite(BigRational),
    PositiveInfinity,
    NegativeInfinity,
    Nan,
}
fn float(value: f64) -> Scalar {
    if let Some(value) = BigRational::from_float(value) {
        Scalar::Finite(value)
    } else if value.is_nan() {
        Scalar::Nan
    } else if value.is_sign_negative() {
        Scalar::NegativeInfinity
    } else {
        Scalar::PositiveInfinity
    }
}
fn numeric(value: &Expr) -> Option<(Scalar, Scalar)> {
    let zero = || Scalar::Finite(BigRational::from_integer(BigInt::from(0)));
    match value {
        Expr::BooleanLiteral(value) => Some((
            Scalar::Finite(BigRational::from_integer(BigInt::from(u8::from(
                value.value,
            )))),
            zero(),
        )),
        Expr::NumberLiteral(value) => Some(match &value.value {
            Number::Int(value) => (
                Scalar::Finite(BigRational::from_integer(value.to_string().parse().ok()?)),
                zero(),
            ),
            Number::Float(value) => (float(*value), zero()),
            Number::Complex { real, imag } => (float(*real), float(*imag)),
        }),
        _ => None,
    }
}
fn equal(left: &Scalar, right: &Scalar) -> bool {
    match (left, right) {
        (Scalar::Finite(left), Scalar::Finite(right)) => left == right,
        (Scalar::PositiveInfinity, Scalar::PositiveInfinity)
        | (Scalar::NegativeInfinity, Scalar::NegativeInfinity) => true,
        _ => false,
    }
}
pub(super) fn constant(value: &Expr) -> bool {
    matches!(
        value,
        Expr::StringLiteral(_)
            | Expr::BytesLiteral(_)
            | Expr::NumberLiteral(_)
            | Expr::BooleanLiteral(_)
            | Expr::NoneLiteral(_)
            | Expr::EllipsisLiteral(_)
    )
}
pub(super) fn same(left: &Expr, right: &Expr) -> bool {
    if let (Some((left_real, left_imag)), Some((right_real, right_imag))) =
        (numeric(left), numeric(right))
    {
        return equal(&left_real, &right_real) && equal(&left_imag, &right_imag);
    }
    match (left, right) {
        (Expr::NoneLiteral(_), Expr::NoneLiteral(_))
        | (Expr::EllipsisLiteral(_), Expr::EllipsisLiteral(_)) => true,
        (Expr::StringLiteral(left), Expr::StringLiteral(right)) => {
            left.value.to_str() == right.value.to_str()
        }
        (Expr::BytesLiteral(left), Expr::BytesLiteral(right)) => {
            left.value.bytes().eq(right.value.bytes())
        }
        _ => false,
    }
}
pub(super) fn value_text(value: &Expr, source: &str) -> String {
    match value {
        Expr::StringLiteral(value) => value.value.to_str().into(),
        Expr::EllipsisLiteral(_) => "Ellipsis".into(),
        _ => super::display::expression(value, source),
    }
}
pub(super) fn bytes_repr(values: &[u8]) -> String {
    let quote = if values.contains(&b'\'') && !values.contains(&b'"') {
        '"'
    } else {
        '\''
    };
    let mut result = format!("b{quote}");
    for &value in values {
        match value {
            b'\t' => result.push_str("\\t"),
            b'\n' => result.push_str("\\n"),
            b'\r' => result.push_str("\\r"),
            b'\\' => result.push_str("\\\\"),
            value if value == quote as u8 => {
                result.push('\\');
                result.push(quote);
            }
            32..=126 => result.push(char::from(value)),
            _ => result.push_str(&format!("\\x{value:02x}")),
        }
    }
    result.push(quote);
    result
}
