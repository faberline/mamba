//! A proper tuple subtype keeps its declared expansion when a field is read.
use super::*;

impl Context {
    pub(in crate::infrastructure::mypy) fn tuple_assignment_type(
        &self,
        expected: Type,
        actual: Type,
    ) -> Type {
        let expected_node = self.node(expected);
        let actual_node = self.node(actual);
        let (Some(Node::Tuple(expected_items)), Some(Node::Tuple(actual_items))) =
            (expected_node.as_deref(), actual_node.as_deref())
        else {
            return actual;
        };
        let Some((prefix, target)) = self.tuple_pack_position(expected_items) else {
            return actual;
        };
        let target_node = self.node(target);
        let Some(Node::VariadicTuple(item)) = target_node.as_deref() else {
            return actual;
        };
        let suffix = expected_items.len() - prefix - 1;
        let Some(middle) = actual_items.len().checked_sub(prefix + suffix) else {
            return actual;
        };
        let expanded = expected_items[..prefix]
            .iter()
            .copied()
            .chain(std::iter::repeat_n(*item, middle))
            .chain(expected_items[prefix + 1..].iter().copied());
        if expanded.zip(actual_items).all(|(declared, &assigned)| {
            !declared.is_any() && !assigned.is_any() && self.accepts(assigned, declared)
        }) {
            expected
        } else {
            actual
        }
    }
}
