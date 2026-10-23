//! Literal values remain distinct from the scalar type used by ordinary inference.
use super::*;
use mamba_language::application::quality_syntax::python_ast::{ExprSubscript, UnaryOp};
use ruff_python_ast::str::TripleQuotes;
use ruff_python_literal::escape::{AsciiEscape, UnicodeEscape};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) enum Literal {
    Int(i64),
    Bool(bool),
    Str(String),
    Bytes(Vec<u8>),
}

impl Literal {
    pub(crate) fn base(&self) -> Type {
        match self {
            Self::Int(_) => Type::Int,
            Self::Bool(_) => Type::Bool,
            Self::Str(_) => Type::Str,
            Self::Bytes(_) => Type::Bytes,
        }
    }

    pub(crate) fn display(&self) -> String {
        match self {
            Self::Int(value) => value.to_string(),
            Self::Bool(value) => if *value { "True" } else { "False" }.into(),
            Self::Str(value) => format!(
                "{}",
                UnicodeEscape::new_repr(value).str_repr(TripleQuotes::No)
            ),
            Self::Bytes(value) => format!(
                "{}",
                AsciiEscape::new_repr(value).bytes_repr(TripleQuotes::No)
            ),
        }
    }
}

impl Context {
    pub(crate) fn literal_condition(
        &self,
        ty: Type,
        target: Type,
        positive: bool,
        identity: bool,
    ) -> Type {
        if let Some(node) = self.node(ty) {
            if let Node::Union(items) = node.as_ref() {
                return self.union(
                    items
                        .iter()
                        .map(|&ty| self.literal_condition(ty, target, positive, identity))
                        .collect(),
                );
            }
        }
        let target_node = self.node(target);
        let Some(Node::Literal(target_value)) = target_node.as_deref() else {
            return self.narrow(ty, target, positive);
        };
        if let Some(node) = self.node(ty) {
            if let Node::Literal(value) = node.as_ref() {
                let equal = value == target_value
                    || (!identity
                        && match (value, target_value) {
                            (Literal::Int(value), Literal::Bool(flag))
                            | (Literal::Bool(flag), Literal::Int(value)) => {
                                *value == i64::from(*flag)
                            }
                            _ => false,
                        });
                return if equal == positive { ty } else { Type::Never };
            }
        }
        // Equality does not prove a value-compatible Any is this literal.
        // Keep identity comparisons on their existing path.
        if ty.is_any() && !identity {
            return ty;
        }
        if ty == target_value.base() || matches!(ty, Type::Any | Type::AnySpecial | Type::Object) {
            if positive {
                return target;
            }
            if ty == Type::Bool {
                if let Literal::Bool(value) = target_value {
                    return self.intern(Node::Literal(Literal::Bool(!value)));
                }
            }
            return ty;
        }
        if positive {
            Type::Never
        } else {
            ty
        }
    }
    pub(crate) fn literal_forms(
        &self,
        body: &[mamba_language::application::quality_syntax::python_ast::Stmt],
        variables: &mut Variables,
    ) {
        for import in crate::infrastructure::project::imports(body, "", false) {
            if matches!(import.module.as_str(), "typing" | "typing_extensions") {
                let name = if import.member.as_deref() == Some("Literal") {
                    Some(import.local)
                } else if import.member.is_none() {
                    Some(format!("{}.Literal", import.local))
                } else {
                    None
                };
                if let Some(name) = name {
                    variables
                        .entry(name)
                        .or_insert_with(|| self.intern(Node::LiteralForm));
                }
            }
        }
    }

    pub(crate) fn is_literal_form(&self, expression: &Expr, variables: &VariableMap) -> bool {
        fn name(expression: &Expr) -> Option<String> {
            match expression {
                Expr::Name(identifier) => Some(identifier.id.to_string()),
                Expr::Attribute(attribute) => {
                    Some(format!("{}.{}", name(&attribute.value)?, attribute.attr))
                }
                _ => None,
            }
        }
        let Some(name) = name(expression) else {
            return false;
        };
        match variables.get(&name).copied() {
            Some(ty) => self
                .node(ty)
                .is_some_and(|node| matches!(node.as_ref(), Node::LiteralForm)),
            None => name.rsplit('.').next() == Some("Literal"),
        }
    }

    pub(in crate::infrastructure::mypy) fn annotation_value(&self, ty: Type) -> Type {
        if self.is_source_self_variable(ty) {
            return ty;
        }
        if self
            .class_id(ty)
            .is_some_and(|id| self.class(id).namespace == "builtins.type")
        {
            return self.intern(Node::TypingForm("builtin:type".into()));
        }
        if self
            .node(ty)
            .is_some_and(|node| matches!(node.as_ref(), Node::LiteralForm))
        {
            Type::Any
        } else {
            self.bare_class_annotation(ty)
        }
    }

    pub(crate) fn literal_expression(&self, expression: &Expr) -> Option<Type> {
        let literal = match expression {
            Expr::NoneLiteral(_) => return Some(Type::None),
            Expr::StringLiteral(string) => Literal::Str(string.value.to_str().into()),
            Expr::BytesLiteral(bytes) => Literal::Bytes(bytes.value.bytes().collect()),
            Expr::BooleanLiteral(boolean) => Literal::Bool(boolean.value),
            Expr::NumberLiteral(number) => {
                let Number::Int(value) = &number.value else {
                    return None;
                };
                Literal::Int(value.as_i64()?)
            }
            Expr::UnaryOp(unary) if unary.op == UnaryOp::USub => {
                let Expr::NumberLiteral(number) = unary.operand.as_ref() else {
                    return None;
                };
                let Number::Int(value) = &number.value else {
                    return None;
                };
                Literal::Int(value.as_i64()?.checked_neg()?)
            }
            Expr::UnaryOp(unary) if unary.op == UnaryOp::UAdd => {
                return self.literal_expression(&unary.operand)
            }
            _ => return None,
        };
        Some(self.intern(Node::Literal(literal)))
    }

    pub(crate) fn contains_literal(&self, ty: Type) -> bool {
        self.node(ty).is_some_and(|node| match node.as_ref() {
            Node::Literal(_) => true,
            Node::Union(items) => items.iter().any(|&ty| self.contains_literal(ty)),
            Node::Variable(variable) => {
                self.contains_literal(variable.bound)
                    || variable.values.iter().any(|&ty| self.contains_literal(ty))
            }
            _ => false,
        })
    }

    pub(super) fn literal_annotation(
        &self,
        arguments: &[&Expr],
        variables: &VariableMap,
    ) -> Option<Type> {
        if arguments.is_empty() {
            return Some(Type::Any);
        }
        let types = arguments
            .iter()
            .map(|argument| {
                self.literal_expression(argument).or_else(|| {
                    let ty = self.annotation_with_variables(argument, variables)?;
                    (ty == Type::None || self.contains_literal(ty)).then_some(ty)
                })
            })
            .collect::<Option<Vec<_>>>();
        Some(types.map(|types| self.union(types)).unwrap_or(Type::Any))
    }

    pub(crate) fn literal_annotation_error(
        &self,
        subscript: &ExprSubscript,
        variables: &VariableMap,
    ) -> Option<String> {
        if !self.is_literal_form(&subscript.value, variables) {
            return None;
        }
        let arguments: Vec<_> = match subscript.slice.as_ref() {
            Expr::Tuple(tuple) => tuple.elts.iter().collect(),
            expression => vec![expression],
        };
        if arguments.is_empty() {
            return Some("Literal[...] must have at least one parameter".into());
        }
        for (index, argument) in arguments.iter().enumerate() {
            if self.literal_expression(argument).is_some() {
                continue;
            }
            if self
                .annotation_with_variables(argument, variables)
                .is_some_and(|ty| ty == Type::None || self.contains_literal(ty))
            {
                continue;
            }
            if self
                .annotation_with_variables(argument, variables)
                .is_some_and(Type::is_any)
            {
                return Some(format!(
                    "Parameter {} of Literal[...] cannot be of type \"Any\"",
                    index + 1
                ));
            }
            return Some(
                if matches!(argument, Expr::NumberLiteral(_) | Expr::UnaryOp(_)) {
                    "Invalid type: Literal[...] cannot contain arbitrary expressions".into()
                } else {
                    format!("Parameter {} of Literal[...] is invalid", index + 1)
                },
            );
        }
        None
    }
}
