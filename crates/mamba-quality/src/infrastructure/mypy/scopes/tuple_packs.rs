//! A variadic tuple argument supplies one packed inference input.
use super::*;
use mamba_language::application::quality_syntax::python_ast::ExprCall;

impl Scopes {
    pub(in crate::infrastructure::mypy) fn attribute_key(expression: &Expr) -> Option<String> {
        match expression {
            Expr::Name(name) => Some(name.id.to_string()),
            Expr::Attribute(attribute) => Some(format!(
                "{}.{}",
                Self::attribute_key(&attribute.value)?,
                attribute.attr
            )),
            Expr::Subscript(subscript) => {
                let index = match subscript.slice.as_ref() {
                    Expr::NumberLiteral(number) => match &number.value {
                        mamba_language::application::quality_syntax::python_ast::Number::Int(
                            value,
                        ) => value.to_string(),
                        _ => return None,
                    },
                    Expr::StringLiteral(string) => format!("{:?}", string.value.to_str()),
                    _ => return None,
                };
                Some(format!(
                    "{}[{index}]",
                    Self::attribute_key(&subscript.value)?
                ))
            }
            _ => None,
        }
    }

    pub(super) fn tuple_call_pair(
        &self,
        call: &ExprCall,
        parameter: &super::super::signatures::Parameter,
        fixed: usize,
        environment: &BTreeMap<String, Type>,
    ) -> Option<(Type, Type)> {
        let target = self.context.unpack_target(parameter.ty?)?;
        let values =
            call.arguments
                .args
                .iter()
                .skip(fixed)
                .map(|value| {
                    let (expression, unpack) = match value {
                        Expr::Starred(starred) => (starred.value.as_ref(), true),
                        expression => (expression, false),
                    };
                    let ty =
                        self.context
                            .infer_with_calls(expression, environment, &|expression| {
                                self.expression_type(expression, environment)
                            })?;
                    Some(if unpack {
                        match self.context.node(ty).as_deref() {
                            Some(types::Node::Tuple(_) | types::Node::VariadicTuple(_)) => {
                                self.context.intern(types::Node::Unpack(ty))
                            }
                            _ => self.context.intern(types::Node::Unpack(self.context.intern(
                                types::Node::VariadicTuple(self.context.iterable_item(ty)?),
                            ))),
                        }
                    } else if self.context.is_tuple_variable(target) {
                        self.context
                            .literal_expression(expression)
                            .and_then(|ty| self.context.node(ty))
                            .and_then(|node| match node.as_ref() {
                                types::Node::Literal(literal) => Some(
                                    self.context
                                        .intern(types::Node::LiteralValue(literal.clone())),
                                ),
                                _ => None,
                            })
                            .unwrap_or(ty)
                    } else {
                        ty
                    })
                })
                .collect::<Option<Vec<_>>>()?;
        Some((target, self.context.normalize_tuple(values)))
    }
}
