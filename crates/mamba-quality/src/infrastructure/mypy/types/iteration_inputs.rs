//! Iteration calls preserve bottom types and tuple element unions.
use super::*;
use crate::infrastructure::mypy::signatures::Signature;

impl Context {
    pub(in crate::infrastructure::mypy) fn iteration_argument(
        &self,
        signature: &Signature,
        expression: &Expr,
        actual: Type,
    ) -> Type {
        if signature.definition_module.as_deref() == Some("builtins")
            && matches!(
                signature.name.as_deref(),
                Some(
                    "list"
                        | "set"
                        | "frozenset"
                        | "tuple"
                        | "reversed"
                        | "enumerate"
                        | "sorted"
                        | "sum"
                        | "zip"
                )
            )
            && matches!(expression, Expr::List(value) if value.elts.is_empty())
        {
            return self.intern(Node::Container(Container::List, vec![Type::Never]));
        }
        if signature.definition_module.as_deref() == Some("builtins")
            && matches!(
                signature.name.as_deref(),
                Some(
                    "set"
                        | "frozenset"
                        | "tuple"
                        | "reversed"
                        | "enumerate"
                        | "sorted"
                        | "sum"
                        | "zip"
                )
            )
            && matches!(expression, Expr::Dict(value) if value.items.is_empty())
        {
            return self.intern(Node::Container(Container::Dict, vec![Type::Never; 2]));
        }
        if signature.definition_module.as_deref() != Some("builtins")
            || !matches!(
                signature.name.as_deref(),
                Some("iter" | "next" | "reversed")
            )
        {
            return actual;
        }
        match expression {
            Expr::List(value) if value.elts.is_empty() => {
                self.intern(Node::Container(Container::List, vec![Type::Never]))
            }
            Expr::Tuple(tuple) => {
                let Some(Node::Tuple(items)) = self.node(actual).as_deref().cloned() else {
                    return actual;
                };
                let items = tuple
                    .elts
                    .iter()
                    .zip(items)
                    .map(|(expression, ty)| {
                        self.literal_expression(expression)
                            .and_then(|ty| self.node(ty))
                            .and_then(|node| match node.as_ref() {
                                Node::Literal(literal) => {
                                    Some(self.intern(Node::LiteralValue(literal.clone())))
                                }
                                _ => None,
                            })
                            .unwrap_or(ty)
                    })
                    .collect();
                self.intern(Node::Tuple(items))
            }
            _ => actual,
        }
    }

    pub(in crate::infrastructure::mypy) fn iteration_assignment(
        &self,
        ty: Type,
        preserve_bottom: bool,
    ) -> Type {
        if let Some(Node::Container(Container::Dict, arguments)) = self.node(ty).as_deref() {
            return self.intern(Node::Container(
                Container::Dict,
                arguments
                    .iter()
                    .map(|&ty| self.iteration_item(ty))
                    .collect(),
            ));
        }
        if let Some(Node::Tuple(items)) = self.node(ty).as_deref() {
            return self.intern(Node::Tuple(
                items.iter().map(|&ty| self.iteration_item(ty)).collect(),
            ));
        }
        let Some(Node::AppliedClass {
            id,
            arguments,
            value: false,
        }) = self.node(ty).as_deref().cloned()
        else {
            return ty;
        };
        if self.class(id).namespace != "typing.Iterator" {
            return ty;
        }
        let arguments = arguments
            .into_iter()
            .map(|ty| {
                if ty == Type::Never && !preserve_bottom {
                    Type::Any
                } else {
                    self.iteration_item(ty)
                }
            })
            .collect();
        self.applied_class(id, arguments, false)
    }

    fn iteration_item(&self, ty: Type) -> Type {
        match self.node(ty).as_deref() {
            Some(Node::LiteralValue(value)) => value.base(),
            Some(Node::Tuple(items)) => self.intern(Node::Tuple(
                items.iter().map(|&ty| self.iteration_item(ty)).collect(),
            )),
            Some(Node::Union(items)) => {
                self.union(items.iter().map(|&ty| self.iteration_item(ty)).collect())
            }
            _ => ty,
        }
    }

    pub(in crate::infrastructure::mypy) fn incomplete_iterator(&self, ty: Type) -> bool {
        self.class_id(ty)
            .is_some_and(|id| self.class(id).namespace == "typing.Iterator")
            && self.class_arguments(ty).contains(&Type::Never)
    }

    pub(in crate::infrastructure::mypy) fn iteration_return(
        &self,
        signature: &Signature,
        ty: Type,
    ) -> Type {
        if signature.native_symbol == Some("builtins.sum") {
            return self.aggregation_return(ty);
        }
        if signature.definition_module.as_deref() == Some("builtins")
            && matches!(signature.name.as_deref(), Some("iter" | "next"))
        {
            if matches!(self.node(ty).as_deref(), Some(Node::Union(items)) if items.contains(&Type::Object))
            {
                return Type::Object;
            }
            self.simplify_inference_union(ty)
        } else {
            ty
        }
    }
}
