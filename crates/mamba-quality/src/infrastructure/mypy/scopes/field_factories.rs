//! Empty builtin factories preserve their container kind and element context.
use super::*;
use crate::infrastructure::mypy::types::{Container, Node};

impl Scopes {
    pub(super) fn empty_field_factory(&self, name: &str, expected: Option<Type>) -> Option<Type> {
        let empty = match name {
            "list" => Node::Container(Container::List, vec![Type::Never]),
            "set" => Node::Container(Container::Set, vec![Type::Never]),
            "dict" => Node::Container(Container::Dict, vec![Type::Never, Type::Never]),
            "frozenset" => Node::Container(Container::FrozenSet, vec![Type::Never]),
            "tuple" => Node::VariadicTuple(Type::Never),
            _ => return None,
        };
        let expected = expected.filter(|&ty| {
            self.context
                .node(ty)
                .is_some_and(|node| match (&empty, node.as_ref()) {
                    (Node::Container(kind, _), Node::Container(target, _)) => kind == target,
                    (Node::VariadicTuple(_), Node::VariadicTuple(_)) => true,
                    _ => false,
                })
        });
        Some(expected.unwrap_or_else(|| self.context.intern(empty)))
    }
}
