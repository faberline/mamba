//! Collection constructors retain their iterable input and generic result types.
use super::super::signatures::{Kind, Parameter, Signature};
use super::*;
use std::sync::Arc;

impl Context {
    pub(in crate::infrastructure::mypy) fn collection_function(
        &self,
        name: &str,
    ) -> Option<Arc<Signature>> {
        let kind = match name {
            "set" => Container::Set,
            "frozenset" => Container::FrozenSet,
            _ => return None,
        };
        if let Some(signature) = self.classes.borrow().collection_functions.get(name) {
            return Some(signature.clone());
        }
        let item = self.variable("_T".into(), Type::Object, Vec::new(), None);
        let mut signature =
            Signature::empty(name.into(), self.intern(Node::Container(kind, vec![item])));
        signature.generic = Arc::from([item]);
        signature.parameters = Arc::from([Parameter {
            name: String::new(),
            ty: Some(self.intern(Node::Container(Container::Iterable, vec![item]))),
            kind: Kind::PositionalOnly,
            required: false,
        }]);
        let signature = Arc::new(signature);
        self.classes
            .borrow_mut()
            .collection_functions
            .insert(name.into(), signature.clone());
        Some(signature)
    }
}
