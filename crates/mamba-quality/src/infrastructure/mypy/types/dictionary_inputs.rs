//! Dictionary constructors retain empty inputs, pair shapes, and literal context.
use super::*;
use crate::infrastructure::mypy::signatures::Signature;

impl Context {
    // A fresh literal's context may widen each homogeneous scalar component.
    // Names, calls, unpacking, empty literals, and nested containers need more proof.
    pub(in crate::infrastructure::mypy::types) fn dictionary_or_literal(
        &self,
        expression: &Expr,
        actual: Type,
    ) -> Option<Vec<Type>> {
        fn scalar(expression: &Expr) -> Option<Type> {
            match expression {
                Expr::StringLiteral(_) => Some(Type::Str),
                Expr::BytesLiteral(_) => Some(Type::Bytes),
                Expr::BooleanLiteral(_) => Some(Type::Bool),
                Expr::NoneLiteral(_) => Some(Type::None),
                Expr::NumberLiteral(number) => Some(match number.value {
                    Number::Int(_) => Type::Int,
                    Number::Float(_) => Type::Float,
                    Number::Complex { .. } => Type::Complex,
                }),
                _ => None,
            }
        }
        let Expr::Dict(dictionary) = expression else {
            return None;
        };
        let first = dictionary.items.first()?;
        let components = vec![scalar(first.key.as_ref()?)?, scalar(&first.value)?];
        if !dictionary.items.iter().all(|item| {
            item.key.as_ref().and_then(scalar) == Some(components[0])
                && scalar(&item.value) == Some(components[1])
        }) || self.dictionary_or_arguments(actual)? != components
        {
            return None;
        }
        Some(components)
    }

    pub(in crate::infrastructure::mypy) fn dictionary_argument(
        &self,
        signature: &Signature,
        expression: &Expr,
        actual: Type,
        infer: &impl Fn(&Expr) -> Option<Type>,
    ) -> Type {
        if signature.definition_module.as_deref() != Some("builtins")
            || signature.name.as_deref() != Some("dict")
        {
            return actual;
        }
        match expression {
            Expr::List(list) if list.elts.is_empty() => {
                self.intern(Node::Container(Container::List, vec![Type::Never]))
            }
            Expr::Dict(dictionary) if dictionary.items.is_empty() => {
                self.intern(Node::Container(Container::Dict, vec![Type::Never; 2]))
            }
            Expr::List(list) => {
                let tuples: Option<Vec<Vec<Type>>> = list
                    .elts
                    .iter()
                    .map(|item| {
                        let ty = infer(item)?;
                        match self.node(ty).as_deref() {
                            Some(Node::Tuple(items)) => Some(items.clone()),
                            _ => None,
                        }
                    })
                    .collect();
                let Some(tuples) = tuples else { return actual };
                let Some(first) = tuples.first() else {
                    return actual;
                };
                if !tuples.iter().all(|tuple| tuple.len() == first.len()) {
                    return actual;
                }
                let items = (0..first.len())
                    .map(|index| {
                        tuples
                            .iter()
                            .map(|items| items[index])
                            .reduce(|left, right| self.join(left, right))
                            .unwrap()
                    })
                    .collect();
                self.intern(Node::Container(
                    Container::List,
                    vec![self.intern(Node::Tuple(items))],
                ))
            }
            Expr::Tuple(tuple) => {
                let Some(Node::Tuple(items)) = self.node(actual).as_deref().cloned() else {
                    return actual;
                };
                let items = tuple
                    .elts
                    .iter()
                    .zip(items)
                    .map(|(item, ty)| self.dictionary_argument(signature, item, ty, infer))
                    .collect();
                self.intern(Node::Tuple(items))
            }
            _ => actual,
        }
    }

    pub(in crate::infrastructure::mypy) fn constructor_contextual_argument(
        &self,
        signature: &Signature,
        expected: Type,
        expression: &Expr,
        actual: Type,
    ) -> Type {
        if self.dictionary_or_signature(signature) {
            if let (Some(expected_arguments), Some(actual_arguments)) = (
                self.dictionary_or_arguments(expected),
                self.dictionary_or_literal(expression, actual),
            ) {
                if expected_arguments
                    .iter()
                    .zip(actual_arguments)
                    .all(|(&expected, actual)| self.accepts(expected, actual))
                {
                    return expected;
                }
            }
        }
        if signature.definition_module.as_deref() == Some("builtins")
            && signature.name.as_deref() == Some("reversed")
            && matches!(expression, Expr::List(_) | Expr::Dict(_))
        {
            return self
                .reversed_literal_context(expected, actual)
                .unwrap_or(actual);
        }
        if signature.definition_module.as_deref() == Some("builtins")
            && signature.name.as_deref() == Some("dict")
            && matches!(expression, Expr::Dict(_))
        {
            if let Some(id) = self.class_id(expected) {
                if self.class(id).namespace == "_typeshed.SupportsKeysAndGetItem" {
                    return self.intern(Node::Container(
                        Container::Dict,
                        self.class_arguments(expected),
                    ));
                }
            }
        }
        actual
    }

    pub(in crate::infrastructure::mypy) fn unresolved_dictionary_pairs(
        &self,
        signature: &Signature,
        expected: Type,
    ) -> bool {
        signature.definition_module.as_deref() == Some("builtins")
            && signature.name.as_deref() == Some("dict")
            && self.collection_literal_item(expected).is_some_and(|item| {
                matches!(self.node(item).as_deref(), Some(Node::Tuple(items)) if items.contains(&Type::Never))
            })
    }
}
