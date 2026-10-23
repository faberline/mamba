//! Immutable constructor results retain unresolved element types until assignment.
use super::*;
use crate::infrastructure::mypy::signatures::Signature;

impl Context {
    pub(in crate::infrastructure::mypy) fn immutable_collection_bottom(
        &self,
        signature: &Signature,
        actual: Type,
    ) -> bool {
        if !signature.constructor
            || signature.generic.is_empty()
            || signature.definition_module.as_deref() != Some("builtins")
        {
            return false;
        }
        match (signature.name.as_deref(), self.node(actual).as_deref()) {
            (
                Some("enumerate"),
                Some(Node::AppliedClass {
                    id,
                    arguments,
                    value: false,
                }),
            ) if self.class(*id).namespace == "builtins.enumerate" => {
                arguments.contains(&Type::Never)
            }
            (Some("tuple"), Some(Node::VariadicTuple(Type::Never))) => true,
            (Some("frozenset"), Some(Node::Container(Container::FrozenSet, arguments))) => {
                arguments.contains(&Type::Never)
            }
            _ => false,
        }
    }

    pub(in crate::infrastructure::mypy) fn collection_constructor_context(
        &self,
        signature: &Signature,
        expected: Type,
    ) -> Option<Type> {
        if signature.definition_module.as_deref() != Some("builtins")
            || signature.name.as_deref() != Some("tuple")
        {
            return Some(expected);
        }
        match self.node(expected).as_deref() {
            Some(Node::Tuple(_)) => None,
            Some(Node::Container(Container::Iterable | Container::Sequence, arguments)) => {
                Some(self.intern(Node::VariadicTuple(arguments[0])))
            }
            _ => self
                .collection_literal_item(expected)
                .map(|item| self.intern(Node::VariadicTuple(item)))
                .or(Some(expected)),
        }
    }
}
