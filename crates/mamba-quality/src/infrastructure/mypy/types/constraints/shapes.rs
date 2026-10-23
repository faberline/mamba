//! Constraint candidates retain callable domains and container shapes.
use super::*;

impl Context {
    pub(in super::super) fn constraint_shape_matches(&self, expected: Type, actual: Type) -> bool {
        let Some(expected) = self.node(expected) else {
            return self.accepts(expected, actual);
        };
        match (expected.as_ref(), self.node(actual).as_deref()) {
            (Node::Variable(_), _) => true,
            (Node::AppliedClass { id, value, .. }, _) => {
                self.is_class_value(actual) == *value
                    && self.class_arguments_for(actual, *id).is_some()
            }
            (Node::Callable(expected), _) => {
                self.callable_signature(actual).is_some_and(|actual| {
                    let accepts = |actual: &super::super::super::signatures::Signature| {
                        crate::infrastructure::mypy::signatures::compatibility::parameters_accept(
                            self, expected, actual, true,
                        )
                    };
                    if actual.overloads.is_empty() {
                        accepts(&actual)
                    } else {
                        actual.overloads.iter().any(|variant| accepts(variant))
                    }
                })
            }
            (Node::Container(Container::Sequence, _), _) => self.sequence_item(actual).is_some(),
            (Node::Container(Container::Awaitable, _), _) => self.await_result(actual).is_some(),
            (Node::Container(Container::TypeObject, _), _) => self.is_class_value(actual),
            (Node::Container(Container::Mapping, _), _) => self.mapping_types(actual).is_some(),
            (Node::Container(kind, _), Some(Node::Container(other, _))) => kind == other,
            (Node::Tuple(_), Some(Node::Tuple(_)))
            | (Node::VariadicTuple(_), Some(Node::Tuple(_) | Node::VariadicTuple(_))) => true,
            _ => false,
        }
    }
}
