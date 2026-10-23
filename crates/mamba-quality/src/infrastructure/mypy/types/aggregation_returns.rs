//! Aggregation unions preserve numeric alternatives without argument promotions.
use super::*;

impl Context {
    pub(super) fn aggregation_return(&self, ty: Type) -> Type {
        let Some(Node::Union(items)) = self.node(ty).as_deref().cloned() else {
            return ty;
        };
        self.union(
            items
                .iter()
                .copied()
                .enumerate()
                .filter_map(|(index, item)| {
                    (!items
                        .iter()
                        .copied()
                        .enumerate()
                        .any(|(other_index, other)| {
                            other_index != index
                                && self.union_subtype(other, item)
                                && (other_index < index || !self.union_subtype(item, other))
                        }))
                    .then_some(item)
                })
                .collect(),
        )
    }

    fn union_subtype(&self, expected: Type, actual: Type) -> bool {
        let scalar = |ty| match self.node(ty).as_deref() {
            Some(Node::Literal(value) | Node::LiteralValue(value)) => value.base(),
            _ => ty,
        };
        if matches!(
            (scalar(expected), scalar(actual)),
            (Type::Float, Type::Int | Type::Bool)
                | (Type::Complex, Type::Int | Type::Bool | Type::Float)
        ) {
            return false;
        }
        self.accepts(expected, actual)
    }
}
