//! A Rust evaluator for Python literal data. It never runs Python source.
use mamba_language::application::quality_syntax::python_ast::{
    str::{Quote, TripleQuotes},
    Expr, Number, Operator, UnaryOp,
};
use num_bigint::BigInt;
use num_rational::BigRational;
use ruff_python_literal::escape::{AsciiEscape, UnicodeEscape};
use std::cmp::Ordering;

#[derive(Clone, PartialEq)]
pub enum OrderValue {
    Number(Numeric),
    String(String),
    Bytes(Vec<u8>),
    Sequence(bool, Vec<Value>),
    Set(Vec<Value>),
    Unordered(String),
}

#[derive(Clone, PartialEq, PartialOrd)]
pub enum Numeric {
    NegativeInfinity,
    Finite(BigRational),
    PositiveInfinity,
}

impl OrderValue {
    fn type_name(&self) -> &'static str {
        match self {
            Self::Number(_) => "int",
            Self::String(_) => "str",
            Self::Bytes(_) => "bytes",
            Self::Sequence(true, _) => "tuple",
            Self::Sequence(false, _) => "list",
            Self::Set(_) => "set",
            Self::Unordered(value) if value == "None" => "NoneType",
            Self::Unordered(value) if value == "Ellipsis" => "ellipsis",
            Self::Unordered(_) => "complex",
        }
    }

    fn unhashable_type(&self) -> Option<&'static str> {
        match self {
            Self::Sequence(false, _) => Some("list"),
            Self::Set(_) => Some("set"),
            Self::Sequence(true, values) => values.iter().find_map(Value::unhashable_type),
            _ => None,
        }
    }

    pub fn compare(&self, other: &Self) -> Option<Ordering> {
        match (self, other) {
            (Self::Number(left), Self::Number(right)) => left.partial_cmp(right),
            (Self::String(left), Self::String(right)) => Some(left.cmp(right)),
            (Self::Bytes(left), Self::Bytes(right)) => Some(left.cmp(right)),
            (Self::Sequence(left_kind, left), Self::Sequence(right_kind, right))
                if left_kind == right_kind =>
            {
                for (left, right) in left.iter().zip(right) {
                    if left == right {
                        continue;
                    }
                    let order = left.order.compare(&right.order)?;
                    if order != Ordering::Equal {
                        return Some(order);
                    }
                }
                Some(left.len().cmp(&right.len()))
            }
            _ => None,
        }
    }
}

#[derive(Clone)]
pub struct Value {
    pub order: OrderValue,
    pub repr: String,
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        self.order == other.order
    }
}

impl Value {
    pub fn unhashable_type(&self) -> Option<&'static str> {
        self.order.unhashable_type()
    }

    fn type_name(&self) -> &'static str {
        if matches!(self.order, OrderValue::Number(_)) {
            if matches!(self.repr.as_str(), "True" | "False") {
                "bool"
            } else if self.repr.contains(['.', 'e']) || self.repr.ends_with("inf") {
                "float"
            } else {
                "int"
            }
        } else {
            self.order.type_name()
        }
    }

    fn comparison_error(&self, other: &Self) -> Option<String> {
        if let (OrderValue::Sequence(left_kind, left), OrderValue::Sequence(right_kind, right)) =
            (&self.order, &other.order)
        {
            if left_kind == right_kind {
                for (left, right) in left.iter().zip(right) {
                    if left != right {
                        return left.comparison_error(right);
                    }
                }
                return None;
            }
        }
        self.order.compare(&other.order).is_none().then(|| {
            format!(
                "'<' not supported between instances of '{}' and '{}'",
                self.type_name(),
                other.type_name()
            )
        })
    }

    pub fn element_repr(&self) -> String {
        if let OrderValue::String(value) = &self.order {
            let quote = if value.contains(['\\', '\n', '\r', '\t']) {
                Quote::Single
            } else {
                Quote::Double
            };
            UnicodeEscape::with_preferred_quote(value, quote)
                .str_repr(TripleQuotes::No)
                .to_string()
                .unwrap()
        } else {
            self.repr.clone()
        }
    }
}

fn integer(number: &str) -> Result<BigInt, String> {
    let number = number.replace('_', "");
    let (radix, digits) = match number.get(..2).map(str::to_ascii_lowercase).as_deref() {
        Some("0x") => (16, &number[2..]),
        Some("0o") => (8, &number[2..]),
        Some("0b") => (2, &number[2..]),
        _ => (10, number.as_str()),
    };
    BigInt::parse_bytes(digits.as_bytes(), radix).ok_or_else(|| "Invalid integer literal".into())
}

fn numeric(number: &Number, negative: bool) -> Result<Value, String> {
    match number {
        Number::Int(value) => {
            let value = integer(&value.to_string())?;
            let value = if negative { -value } else { value };
            Ok(Value {
                repr: value.to_string(),
                order: OrderValue::Number(Numeric::Finite(BigRational::from_integer(value))),
            })
        }
        Number::Float(value) => {
            let value = if negative { -*value } else { *value };
            let order = BigRational::from_float(value)
                .map(Numeric::Finite)
                .unwrap_or_else(|| {
                    if value.is_sign_negative() {
                        Numeric::NegativeInfinity
                    } else {
                        Numeric::PositiveInfinity
                    }
                });
            Ok(Value {
                repr: ruff_python_literal::float::to_string(value),
                order: OrderValue::Number(order),
            })
        }
        Number::Complex { real, imag } => {
            let real = if negative { -*real } else { *real };
            let imag = if negative { -*imag } else { *imag };
            let part = |value: f64| {
                ruff_python_literal::float::to_string(value)
                    .trim_end_matches(".0")
                    .to_owned()
            };
            let repr = if real == 0.0 {
                format!("{}j", part(imag))
            } else {
                format!(
                    "({}{}{}j)",
                    part(real),
                    if imag.is_sign_negative() { "" } else { "+" },
                    part(imag)
                )
            };
            Ok(Value {
                order: OrderValue::Unordered(repr.clone()),
                repr,
            })
        }
    }
}

pub fn evaluate(expression: &Expr) -> Result<Value, String> {
    match expression {
        Expr::StringLiteral(value) => {
            let value = value.value.to_str().to_owned();
            let repr = UnicodeEscape::new_repr(&value)
                .str_repr(TripleQuotes::No)
                .to_string()
                .unwrap();
            Ok(Value {
                order: OrderValue::String(value),
                repr,
            })
        }
        Expr::BytesLiteral(value) => {
            let value: Vec<_> = value.value.bytes().collect();
            let repr = AsciiEscape::new_repr(&value)
                .bytes_repr(TripleQuotes::No)
                .to_string()
                .unwrap();
            Ok(Value {
                order: OrderValue::Bytes(value),
                repr,
            })
        }
        Expr::NumberLiteral(value) => numeric(&value.value, false),
        Expr::BinOp(value) if matches!(value.op, Operator::Add | Operator::Sub) => {
            let left =
                numeric_value(&value.left).ok_or("The sorting directive requires literal data")?;
            let Expr::NumberLiteral(right) = value.right.as_ref() else {
                return Err("The sorting directive requires literal data".into());
            };
            let Number::Complex { real, imag } = right.value else {
                return Err("The sorting directive requires literal data".into());
            };
            if real != 0.0 {
                return Err("The sorting directive requires literal data".into());
            }
            numeric(
                &Number::Complex {
                    real: left,
                    imag: if value.op == Operator::Sub {
                        -imag
                    } else {
                        imag
                    },
                },
                false,
            )
        }
        Expr::UnaryOp(value) if matches!(value.op, UnaryOp::UAdd | UnaryOp::USub) => {
            let Expr::NumberLiteral(number) = value.operand.as_ref() else {
                return Err("Unary literal requires a number".into());
            };
            numeric(&number.value, value.op == UnaryOp::USub)
        }
        Expr::BooleanLiteral(value) => Ok(Value {
            repr: if value.value { "True" } else { "False" }.into(),
            order: OrderValue::Number(Numeric::Finite(BigRational::from_integer(BigInt::from(
                u8::from(value.value),
            )))),
        }),
        Expr::NoneLiteral(_) => Ok(Value {
            repr: "None".into(),
            order: OrderValue::Unordered("None".into()),
        }),
        Expr::EllipsisLiteral(_) => Ok(Value {
            repr: "Ellipsis".into(),
            order: OrderValue::Unordered("Ellipsis".into()),
        }),
        Expr::Set(set) => {
            let mut values = Vec::new();
            for element in &set.elts {
                let value = evaluate(element)?;
                if !values.contains(&value) {
                    values.push(value);
                }
            }
            let repr = if values.is_empty() {
                "set()".into()
            } else {
                format!(
                    "{{{}}}",
                    values
                        .iter()
                        .map(|value| value.repr.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            };
            Ok(Value {
                order: OrderValue::Set(values),
                repr,
            })
        }
        Expr::Call(call)
            if call.arguments.is_empty()
                && matches!(call.func.as_ref(), Expr::Name(name) if name.id == "set") =>
        {
            Ok(Value {
                order: OrderValue::Set(Vec::new()),
                repr: "set()".into(),
            })
        }
        Expr::List(value) => sequence(&value.elts, false),
        Expr::Tuple(value) => sequence(&value.elts, true),
        _ => Err("The sorting directive requires literal data".into()),
    }
}

fn numeric_value(expression: &Expr) -> Option<f64> {
    match expression {
        Expr::NumberLiteral(value) => match &value.value {
            Number::Int(value) => value.to_string().parse().ok(),
            Number::Float(value) => Some(*value),
            _ => None,
        },
        Expr::UnaryOp(value)
            if matches!(value.op, UnaryOp::UAdd | UnaryOp::USub)
                && value.operand.is_number_literal_expr() =>
        {
            numeric_value(&value.operand).map(|number| {
                if value.op == UnaryOp::USub {
                    -number
                } else {
                    number
                }
            })
        }
        _ => None,
    }
}

fn sequence(elements: &[Expr], tuple: bool) -> Result<Value, String> {
    let values: Vec<_> = elements.iter().map(evaluate).collect::<Result<_, _>>()?;
    let mut inner = values
        .iter()
        .map(|value| value.repr.clone())
        .collect::<Vec<_>>()
        .join(", ");
    if tuple && values.len() == 1 {
        inner.push(',');
    }
    Ok(Value {
        repr: if tuple {
            format!("({inner})")
        } else {
            format!("[{inner}]")
        },
        order: OrderValue::Sequence(tuple, values),
    })
}

pub fn sorted(mut values: Vec<Value>, unique: bool) -> Result<Vec<Value>, String> {
    if unique {
        if let Some(kind) = values.iter().find_map(Value::unhashable_type) {
            return Err(format!("unhashable type: '{kind}'"));
        }
        let mut seen = Vec::new();
        values.retain(|value| {
            if seen.contains(&value.order) {
                false
            } else {
                seen.push(value.order.clone());
                true
            }
        });
    }
    if values.len() > 1 {
        for pair in values.windows(2) {
            if let Some(message) = pair[1].comparison_error(&pair[0]) {
                return Err(message);
            }
        }
    }
    values.sort_by(|left, right| left.order.compare(&right.order).unwrap_or(Ordering::Equal));
    Ok(values)
}
