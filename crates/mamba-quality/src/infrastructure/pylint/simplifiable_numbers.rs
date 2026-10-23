//! Constant numeric reductions keep integer precision and invalid-operation boundaries.
use mamba_language::application::quality_syntax::python_ast::{Expr, Number, Operator, UnaryOp};
use num_bigint::BigInt;
use num_rational::BigRational;

#[derive(Clone)]
pub(super) enum Numeric {
    Integer(BigInt),
    Float(f64),
    Complex(f64, f64),
}
impl Numeric {
    pub(super) fn truth(&self) -> bool {
        match self {
            Self::Integer(value) => *value != BigInt::from(0),
            Self::Float(value) => *value != 0.0,
            Self::Complex(real, imag) => *real != 0.0 || *imag != 0.0,
        }
    }
    fn float(&self) -> Option<f64> {
        match self {
            Self::Integer(value) => value.to_string().parse().ok(),
            Self::Float(value) => Some(*value),
            Self::Complex(_, _) => None,
        }
    }
    pub(super) fn compare(&self, other: &Self) -> Option<std::cmp::Ordering> {
        match (self, other) {
            (Self::Integer(left), Self::Integer(right)) => Some(left.cmp(right)),
            (Self::Integer(left), Self::Float(right)) => integer_float_order(left, *right),
            (Self::Float(left), Self::Integer(right)) => {
                integer_float_order(right, *left).map(std::cmp::Ordering::reverse)
            }
            _ => self.float()?.partial_cmp(&other.float()?),
        }
    }
}

fn integer_float_order(left: &BigInt, right: f64) -> Option<std::cmp::Ordering> {
    if right.is_nan() {
        None
    } else if right == f64::INFINITY {
        Some(std::cmp::Ordering::Less)
    } else if right == f64::NEG_INFINITY {
        Some(std::cmp::Ordering::Greater)
    } else {
        Some(BigRational::from_integer(left.clone()).cmp(&BigRational::from_float(right)?))
    }
}

pub(super) fn number(value: &Expr) -> Option<Numeric> {
    match value {
        Expr::BooleanLiteral(value) => Some(Numeric::Integer(u8::from(value.value).into())),
        Expr::NumberLiteral(value) => Some(match &value.value {
            Number::Int(value) => Numeric::Integer(value.to_string().parse().ok()?),
            Number::Float(value) => Numeric::Float(*value),
            Number::Complex { real, imag } => Numeric::Complex(*real, *imag),
        }),
        Expr::UnaryOp(value) => {
            let number = number(&value.operand)?;
            match (value.op, number) {
                (UnaryOp::UAdd, value) => Some(value),
                (UnaryOp::USub, Numeric::Integer(value)) => Some(Numeric::Integer(-value)),
                (UnaryOp::USub, Numeric::Float(value)) => Some(Numeric::Float(-value)),
                (UnaryOp::USub, Numeric::Complex(real, imag)) => {
                    Some(Numeric::Complex(-real, -imag))
                }
                (UnaryOp::Invert, Numeric::Integer(value)) => Some(Numeric::Integer(!value)),
                _ => None,
            }
        }
        Expr::BinOp(value) => binary(number(&value.left)?, value.op, number(&value.right)?),
        _ => None,
    }
}
fn binary(left: Numeric, op: Operator, right: Numeric) -> Option<Numeric> {
    if let (Numeric::Integer(left), Numeric::Integer(right)) = (&left, &right) {
        let result = match op {
            Operator::Add => Some(left + right),
            Operator::Sub => Some(left - right),
            Operator::Mult => Some(left * right),
            Operator::BitAnd => Some(left & right),
            Operator::BitOr => Some(left | right),
            Operator::BitXor => Some(left ^ right),
            Operator::FloorDiv | Operator::Mod if *right != BigInt::from(0) => {
                let mut quotient = left / right;
                let remainder = left % right;
                if remainder != BigInt::from(0) && remainder.sign() != right.sign() {
                    quotient -= 1;
                }
                Some(if op == Operator::Mod {
                    left - quotient * right
                } else {
                    quotient
                })
            }
            _ => None,
        };
        if let Some(value) = result {
            return Some(Numeric::Integer(value));
        }
    }
    let left = left.float()?;
    let right = right.float()?;
    let value = match op {
        Operator::Add => left + right,
        Operator::Sub => left - right,
        Operator::Mult => left * right,
        Operator::Div if right != 0.0 => left / right,
        Operator::FloorDiv if right != 0.0 => (left / right).floor(),
        Operator::Mod if right != 0.0 => {
            let remainder = left % right;
            if remainder != 0.0 && remainder.is_sign_negative() != right.is_sign_negative() {
                remainder + right
            } else {
                remainder
            }
        }
        Operator::Pow if !(left == 0.0 && right < 0.0) && !(left < 0.0 && right.fract() != 0.0) => {
            left.powf(right)
        }
        _ => return None,
    };
    value.is_finite().then_some(Numeric::Float(value))
}
