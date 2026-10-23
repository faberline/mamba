//! Static values are data; projects and stubs are never executed.
use mamba_language::application::quality_syntax::python_ast::{Expr, Number};

#[derive(Clone)]
pub(super) enum Value {
    Sequence(Vec<Value>, &'static str),
    Mapping(Vec<(Option<Value>, Value)>),
    String(String),
    Bytes(Vec<u8>),
    Number(Number),
    Boolean(bool),
    StringMethod(String, Box<Value>),
    Scalar(&'static str),
    Reference(Vec<String>),
    Spread(Box<Value>),
    Unknown,
}

pub(super) fn path(expression: &Expr) -> Option<Vec<String>> {
    match expression {
        Expr::Name(name) => Some(vec![name.id.to_string()]),
        Expr::Attribute(attribute) => {
            let mut names = path(&attribute.value)?;
            names.push(attribute.attr.to_string());
            Some(names)
        }
        _ => None,
    }
}

impl Value {
    pub(super) fn builtin_name(&self) -> Option<&str> {
        match self {
            Self::Scalar(name) => Some(name),
            Self::String(_) => Some("str"),
            Self::Bytes(_) => Some("bytes"),
            Self::Number(number) => Some(match number {
                Number::Int(_) => "int",
                Number::Float(_) => "float",
                Number::Complex { .. } => "complex",
            }),
            Self::Boolean(_) => Some("bool"),
            _ => None,
        }
    }
    pub(super) fn from_expression(expression: &Expr) -> Self {
        match expression {
            Expr::Tuple(tuple) => Self::Sequence(
                tuple.elts.iter().map(Self::from_expression).collect(),
                "tuple",
            ),
            Expr::List(list) => Self::Sequence(
                list.elts.iter().map(Self::from_expression).collect(),
                "list",
            ),
            Expr::Set(set) => {
                Self::Sequence(set.elts.iter().map(Self::from_expression).collect(), "set")
            }
            Expr::Dict(dict) => Self::Mapping(
                dict.items
                    .iter()
                    .map(|item| {
                        (
                            item.key.as_ref().map(Self::from_expression),
                            Self::from_expression(&item.value),
                        )
                    })
                    .collect(),
            ),
            Expr::StringLiteral(string) => Self::String(string.value.to_str().to_string()),
            Expr::NumberLiteral(number) => Self::Number(number.value.clone()),
            Expr::BooleanLiteral(value) => Self::Boolean(value.value),
            Expr::NoneLiteral(_) => Self::Scalar("NoneType"),
            Expr::BytesLiteral(value) => Self::Bytes(value.value.bytes().collect()),
            Expr::Attribute(attribute)
                if matches!(
                    attribute.attr.as_str(),
                    "format" | "strip" | "lstrip" | "rstrip"
                ) =>
            {
                Self::StringMethod(
                    attribute.attr.to_string(),
                    Box::new(Self::from_expression(&attribute.value)),
                )
            }
            Expr::Starred(starred) => Self::Spread(Box::new(Self::from_expression(&starred.value))),
            Expr::Name(_) | Expr::Attribute(_) => path(expression)
                .map(Self::Reference)
                .unwrap_or(Self::Unknown),
            _ => Self::Unknown,
        }
    }
}
