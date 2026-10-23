//! Expression conversion retains CPython class names and field contexts.
use super::{
    ast::{Builder, Result},
    tree::{self, Value},
};
use ruff_python_ast::{Expr, Number};

impl Builder<'_> {
    pub fn expr(&self, expression: &Expr) -> Result {
        use tree::{constant, integer, leaf, node, none, string};
        Ok(match expression {
            Expr::BoolOp(v) => node(
                "BoolOp",
                [
                    ("op", leaf(format!("{:?}", v.op))),
                    ("values", self.exprs(&v.values)?),
                ],
            ),
            Expr::Named(v) => node(
                "NamedExpr",
                [
                    ("target", self.expr(&v.target)?),
                    ("value", self.expr(&v.value)?),
                ],
            ),
            Expr::BinOp(v) => node(
                "BinOp",
                [
                    ("left", self.expr(&v.left)?),
                    ("op", leaf(format!("{:?}", v.op))),
                    ("right", self.expr(&v.right)?),
                ],
            ),
            Expr::UnaryOp(v) => node(
                "UnaryOp",
                [
                    ("op", leaf(format!("{:?}", v.op))),
                    ("operand", self.expr(&v.operand)?),
                ],
            ),
            Expr::Lambda(v) => node(
                "Lambda",
                [
                    ("args", self.parameters(v.parameters.as_deref())?),
                    ("body", self.expr(&v.body)?),
                ],
            ),
            Expr::If(v) => node(
                "IfExp",
                [
                    ("test", self.expr(&v.test)?),
                    ("body", self.expr(&v.body)?),
                    ("orelse", self.expr(&v.orelse)?),
                ],
            ),
            Expr::Dict(v) => node(
                "Dict",
                [
                    (
                        "keys",
                        Value::List(
                            v.items
                                .iter()
                                .map(|item| self.expr_optional(item.key.as_ref()))
                                .collect::<std::result::Result<Vec<_>, _>>()?,
                        ),
                    ),
                    (
                        "values",
                        self.exprs(v.items.iter().map(|item| &item.value))?,
                    ),
                ],
            ),
            Expr::Set(v) => node("Set", [("elts", self.exprs(&v.elts)?)]),
            Expr::ListComp(v) => node(
                "ListComp",
                [
                    ("elt", self.expr(&v.elt)?),
                    ("generators", self.comprehensions(&v.generators)?),
                ],
            ),
            Expr::SetComp(v) => node(
                "SetComp",
                [
                    ("elt", self.expr(&v.elt)?),
                    ("generators", self.comprehensions(&v.generators)?),
                ],
            ),
            Expr::DictComp(v) => node(
                "DictComp",
                [
                    ("key", self.expr_optional(v.key.as_deref())?),
                    ("value", self.expr(&v.value)?),
                    ("generators", self.comprehensions(&v.generators)?),
                ],
            ),
            Expr::Generator(v) => node(
                "GeneratorExp",
                [
                    ("elt", self.expr(&v.elt)?),
                    ("generators", self.comprehensions(&v.generators)?),
                ],
            ),
            Expr::Await(v) => node("Await", [("value", self.expr(&v.value)?)]),
            Expr::Yield(v) => node(
                "Yield",
                [("value", self.expr_optional(v.value.as_deref())?)],
            ),
            Expr::YieldFrom(v) => node("YieldFrom", [("value", self.expr(&v.value)?)]),
            Expr::Compare(v) => node(
                "Compare",
                [
                    ("left", self.expr(&v.operands[0])?),
                    (
                        "ops",
                        Value::List(v.ops.iter().map(|op| leaf(format!("{op:?}"))).collect()),
                    ),
                    ("comparators", self.exprs(&v.operands[1..])?),
                ],
            ),
            Expr::Call(v) => {
                let (args, keywords) = self.arguments(Some(&v.arguments))?;
                node(
                    "Call",
                    [
                        ("func", self.expr(&v.func)?),
                        ("args", args),
                        ("keywords", keywords),
                    ],
                )
            }
            Expr::FString(v) => self.fstring(&v.value)?,
            Expr::TString(v) => self.tstring(&v.value)?,
            Expr::StringLiteral(v) => constant(string(v.value.to_str())),
            Expr::BytesLiteral(v) => {
                use ruff_python_ast::str::TripleQuotes;
                let bytes: Vec<u8> = v.value.bytes().collect();
                let repr = ruff_python_literal::escape::AsciiEscape::new_repr(&bytes)
                    .bytes_repr(TripleQuotes::No)
                    .to_string()
                    .ok_or("AST bytes representation is too large")?;
                constant(Value::Scalar(repr, "bytes"))
            }
            Expr::NumberLiteral(v) => constant(match &v.value {
                Number::Int(value) => integer(value),
                Number::Float(value) => {
                    Value::Scalar(ruff_python_literal::float::to_string(*value), "float")
                }
                Number::Complex { real, imag } => {
                    let im = ruff_python_literal::float::to_string(*imag);
                    let im = im.strip_suffix(".0").unwrap_or(&im);
                    let value = if *real == 0.0 {
                        format!("{im}j")
                    } else {
                        let re = ruff_python_literal::float::to_string(*real);
                        format!("({re}{}{im}j)", if *imag < 0.0 { "" } else { "+" })
                    };
                    Value::Scalar(value, "complex")
                }
            }),
            Expr::BooleanLiteral(v) => constant(Value::Scalar(
                if v.value { "True" } else { "False" }.into(),
                "bool",
            )),
            Expr::NoneLiteral(_) => constant(none()),
            Expr::EllipsisLiteral(_) => constant(Value::Scalar("Ellipsis".into(), "ellipsis")),
            Expr::Attribute(v) => node(
                "Attribute",
                [
                    ("value", self.expr(&v.value)?),
                    ("attr", string(v.attr.to_string())),
                    ("ctx", context(v.ctx)?),
                ],
            ),
            Expr::Subscript(v) => node(
                "Subscript",
                [
                    ("value", self.expr(&v.value)?),
                    ("slice", self.expr(&v.slice)?),
                    ("ctx", context(v.ctx)?),
                ],
            ),
            Expr::Starred(v) => node(
                "Starred",
                [("value", self.expr(&v.value)?), ("ctx", context(v.ctx)?)],
            ),
            Expr::Name(v) => node(
                "Name",
                [("id", string(v.id.to_string())), ("ctx", context(v.ctx)?)],
            ),
            Expr::List(v) => node(
                "List",
                [("elts", self.exprs(&v.elts)?), ("ctx", context(v.ctx)?)],
            ),
            Expr::Tuple(v) => node(
                "Tuple",
                [("elts", self.exprs(&v.elts)?), ("ctx", context(v.ctx)?)],
            ),
            Expr::Slice(v) => node(
                "Slice",
                [
                    ("lower", self.expr_optional(v.lower.as_deref())?),
                    ("upper", self.expr_optional(v.upper.as_deref())?),
                    ("step", self.expr_optional(v.step.as_deref())?),
                ],
            ),
            Expr::IpyEscapeCommand(_) => {
                return Err("IPython escape expressions have no CPython AST".into())
            }
        })
    }
    fn comprehensions(&self, values: &[ruff_python_ast::Comprehension]) -> Result {
        Ok(Value::List(
            values
                .iter()
                .map(|v| {
                    Ok(tree::node(
                        "comprehension",
                        [
                            ("target", self.expr(&v.target)?),
                            ("iter", self.expr(&v.iter)?),
                            ("ifs", self.exprs(&v.ifs)?),
                            ("is_async", tree::integer(u8::from(v.is_async))),
                        ],
                    ))
                })
                .collect::<std::result::Result<Vec<_>, String>>()?,
        ))
    }
}

fn context(value: ruff_python_ast::ExprContext) -> Result {
    match value {
        ruff_python_ast::ExprContext::Invalid => {
            Err("Invalid expression context has no CPython AST".into())
        }
        _ => Ok(tree::leaf(format!("{value:?}"))),
    }
}
