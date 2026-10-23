//! Reverse overload erasure checks member presence before full protocol inference.
use super::*;
use crate::infrastructure::mypy::signatures::Signature;

impl Context {
    pub(in crate::infrastructure::mypy) fn reversed_erased_argument(
        &self,
        signature: &Signature,
        expected: Type,
        actual: Type,
    ) -> Option<bool> {
        if signature.definition_module.as_deref() != Some("builtins")
            || signature.name.as_deref() != Some("reversed")
        {
            return None;
        }
        if actual.is_any() {
            return Some(true);
        }
        let id = self.class_id(expected)?;
        let members: &[&str] = match self.class(id).namespace.as_str() {
            "builtins._SupportsReversed" => &["__reversed__"],
            "_typeshed.SupportsLenAndGetItem" => &["__len__", "__getitem__"],
            _ => return None,
        };
        if !matches!(
            self.node(actual).as_deref(),
            Some(Node::Container(Container::Mapping, _))
        ) && !self
            .class_id(actual)
            .is_some_and(|id| !self.class(id).generic.is_empty() && !self.is_class_value(actual))
        {
            return None;
        }
        Some(
            members
                .iter()
                .all(|name| self.class_method(actual, name).is_some()),
        )
    }

    pub(in crate::infrastructure::mypy) fn reversed_literal_context(
        &self,
        expected: Type,
        actual: Type,
    ) -> Option<Type> {
        let id = self.class_id(expected)?;
        if self.class(id).namespace != "builtins._SupportsReversed" {
            return None;
        }
        let item = self.collection_literal_item(expected)?;
        match self.node(actual).as_deref()? {
            Node::Container(Container::List, _) => {
                Some(self.intern(Node::Container(Container::List, vec![item])))
            }
            Node::Container(Container::Dict, arguments) => {
                Some(self.intern(Node::Container(Container::Dict, vec![item, arguments[1]])))
            }
            _ => None,
        }
    }
}
