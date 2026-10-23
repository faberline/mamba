//! Covariant sequence compatibility checks each tuple element separately.
use super::*;

impl Context {
    pub(super) fn tuple_element_accepts(&self, expected: Type, actual: Type) -> Option<bool> {
        let Some(Node::Tuple(items)) = self.node(actual).as_deref().cloned() else {
            return None;
        };
        Some(items.into_iter().all(|item| self.accepts(expected, item)))
    }

    pub(super) fn iterable_input_accepts(
        &self,
        kind: Container,
        expected: Type,
        actual: Type,
    ) -> bool {
        self.tuple_element_accepts(expected, actual)
            .unwrap_or_else(|| {
                let item = if kind == Container::Sequence {
                    self.sequence_item(actual)
                } else {
                    self.iterable_item(actual)
                };
                item.is_some_and(|item| self.accepts(expected, item))
            })
    }
}
