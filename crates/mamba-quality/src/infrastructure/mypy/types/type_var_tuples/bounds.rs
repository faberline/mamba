//! Tuple bounds preserve slots or use the original homogeneous fallback.
use super::*;

impl Context {
    pub(in crate::infrastructure::mypy::types) fn join_tuple_bounds(
        &self,
        left: Type,
        right: Type,
    ) -> Type {
        if left.is_any() || right.is_any() {
            return left.merge_any(right);
        }
        if left == right {
            return left;
        }
        match (self.node(left).as_deref(), self.node(right).as_deref()) {
            (Some(Node::Tuple(left)), Some(Node::Tuple(right))) if left.len() == right.len() => {
                self.normalize_tuple(
                    left.iter()
                        .zip(right)
                        .map(|(&left, &right)| self.join(left, right))
                        .collect(),
                )
            }
            (Some(Node::Tuple(left)), Some(Node::Tuple(right))) => {
                let (first, second) = if left.len() <= right.len() {
                    (left, right)
                } else {
                    (right, left)
                };
                let first = self.simplify_inference_union(self.union(first.clone()));
                let second = self.simplify_inference_union(self.union(second.clone()));
                let item = if self.accepts(first, second) {
                    first
                } else if self.accepts(second, first) {
                    second
                } else {
                    self.simplify_inference_union(self.union(vec![first, second]))
                };
                self.intern(Node::VariadicTuple(item))
            }
            (Some(Node::VariadicTuple(left)), Some(Node::VariadicTuple(right))) => {
                self.intern(Node::VariadicTuple(self.join(*left, *right)))
            }
            (Some(Node::Tuple(items)), Some(Node::VariadicTuple(item)))
            | (Some(Node::VariadicTuple(item)), Some(Node::Tuple(items))) => {
                let fixed = self.simplify_inference_union(self.union(items.clone()));
                self.intern(Node::VariadicTuple(self.join(fixed, *item)))
            }
            _ => self.join(left, right),
        }
    }

    pub(in crate::infrastructure::mypy::types) fn meet_tuple_bounds(
        &self,
        left: Type,
        right: Type,
    ) -> Type {
        if left == right {
            return left;
        }
        match (self.node(left).as_deref(), self.node(right).as_deref()) {
            (Some(Node::Tuple(left)), Some(Node::Tuple(right))) if left.len() == right.len() => {
                self.normalize_tuple(
                    left.iter()
                        .zip(right)
                        .map(|(&left, &right)| self.meet_parameter_types(left, right))
                        .collect(),
                )
            }
            (Some(Node::Tuple(_)), Some(Node::Tuple(_))) => {
                self.intern(Node::VariadicTuple(Type::Never))
            }
            (Some(Node::VariadicTuple(left)), Some(Node::VariadicTuple(right))) => self.intern(
                Node::VariadicTuple(self.meet_parameter_types(*left, *right)),
            ),
            (Some(Node::Tuple(items)), Some(Node::VariadicTuple(item)))
            | (Some(Node::VariadicTuple(item)), Some(Node::Tuple(items))) => self.normalize_tuple(
                items
                    .iter()
                    .map(|&ty| self.meet_parameter_types(ty, *item))
                    .collect(),
            ),
            _ => self.meet_parameter_types(left, right),
        }
    }
}
