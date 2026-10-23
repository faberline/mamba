//! Integer folding preserves precision and Python floor and remainder signs.
use mamba_language::application::quality_syntax::python_ast::Operator;
use num_bigint::BigInt;

pub(super) enum Numeric {
    Integer(BigInt),
    Float(f64),
}
impl Numeric {
    fn float(&self) -> Option<f64> {
        match self {
            Self::Integer(integer) => integer.to_string().parse().ok(),
            Self::Float(value) => Some(*value),
        }
    }
}

pub(super) fn integer(text: &str) -> Option<BigInt> {
    let text = text.replace('_', "");
    let (base, digits) = match text.get(..2).map(str::to_ascii_lowercase).as_deref() {
        Some("0x") => (16, &text[2..]),
        Some("0o") => (8, &text[2..]),
        Some("0b") => (2, &text[2..]),
        _ => (10, text.as_str()),
    };
    BigInt::parse_bytes(digits.as_bytes(), base)
}

pub(super) fn binary(left: Numeric, op: Operator, right: Numeric) -> Option<Numeric> {
    if let (Numeric::Integer(left), Numeric::Integer(right)) = (&left, &right) {
        let zero = BigInt::from(0);
        let result = match op {
            Operator::Add => left + right,
            Operator::Sub => left - right,
            Operator::Mult => left * right,
            Operator::BitAnd => left & right,
            Operator::BitOr => left | right,
            Operator::BitXor => left ^ right,
            Operator::FloorDiv | Operator::Mod if *right != zero => {
                let mut quotient = left / right;
                let remainder = left % right;
                if remainder != zero && remainder.sign() != right.sign() {
                    quotient -= 1;
                }
                if op == Operator::Mod {
                    left - quotient * right
                } else {
                    quotient
                }
            }
            Operator::Pow if *right >= zero => left.pow(right.to_string().parse::<u32>().ok()?),
            Operator::LShift if *right >= zero => {
                left << right.to_string().parse::<usize>().ok()?
            }
            Operator::RShift if *right >= zero => {
                left >> right.to_string().parse::<usize>().ok()?
            }
            Operator::Div if *right != zero => {
                return Some(Numeric::Float(
                    left.to_string().parse::<f64>().ok()?
                        / right.to_string().parse::<f64>().ok()?,
                ))
            }
            _ => return None,
        };
        return Some(Numeric::Integer(result));
    }
    let left_value = left.float()?;
    let right_value = right.float()?;
    let result = match op {
        Operator::Add => left_value + right_value,
        Operator::Sub => left_value - right_value,
        Operator::Mult => left_value * right_value,
        Operator::Div if right_value != 0.0 => left_value / right_value,
        Operator::FloorDiv if right_value != 0.0 => (left_value / right_value).floor(),
        Operator::Mod if right_value != 0.0 => {
            let remainder = left_value % right_value;
            if remainder != 0.0 && remainder.is_sign_negative() != right_value.is_sign_negative() {
                remainder + right_value
            } else {
                remainder
            }
        }
        Operator::Pow
            if left_value > 0.0 || (left_value < 0.0 && matches!(right, Numeric::Integer(_))) =>
        {
            let result = left_value.powf(right_value);
            if result.is_infinite() {
                return None;
            }
            result
        }
        _ => return None,
    };
    Some(Numeric::Float(result))
}
