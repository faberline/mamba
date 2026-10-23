//! Flow intersections differ from assignment compatibility for numeric types.
use super::*;

impl Context {
    pub(crate) fn narrow(&self, ty: Type, target: Type, positive: bool) -> Type {
        if let Some(node) = self.node(ty) {
            if let Node::Union(items) = node.as_ref() {
                return self.union(
                    items
                        .iter()
                        .map(|&item| self.narrow(item, target, positive))
                        .collect(),
                );
            }
        }
        if let Some(node) = self.node(target) {
            if let Node::Union(items) = node.as_ref() {
                if positive {
                    return self.union(
                        items
                            .iter()
                            .map(|&target| self.narrow(ty, target, true))
                            .collect(),
                    );
                }
                return items
                    .iter()
                    .fold(ty, |ty, &target| self.narrow(ty, target, false));
            }
        }
        if ty == Type::Never {
            return ty;
        }
        if let Some(node) = self.node(ty) {
            if let Node::Literal(value) = node.as_ref() {
                let covered = self.narrow(value.base(), target, true) == value.base();
                return if covered == positive { ty } else { Type::Never };
            }
        }
        if matches!(ty, Type::Any | Type::AnySpecial | Type::Object) {
            return if positive { target } else { ty };
        }
        let covered =
            target == Type::Object || ty == target || (target == Type::Int && ty == Type::Bool);
        if !positive {
            return if covered { Type::Never } else { ty };
        }
        if covered {
            return ty;
        }
        if ty == Type::Int && target == Type::Bool {
            return Type::Bool;
        }
        Type::Never
    }
}
