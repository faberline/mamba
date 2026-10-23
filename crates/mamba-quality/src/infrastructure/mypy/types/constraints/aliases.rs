//! Recursive constraints preserve binders and stop at an active alias pair.
use super::*;

impl Context {
    pub(super) fn alias_constraints(
        &self,
        expected: Type,
        actual: Type,
        variance: Variance,
        contextual: bool,
        bounds: &mut BTreeMap<Type, Bounds>,
    ) -> bool {
        if actual == Type::Never && self.alias_application(expected).is_some() {
            return true;
        }
        if !self.is_alias_instance(expected) && !self.is_alias_instance(actual) {
            return false;
        }
        let key = (
            expected,
            actual,
            match variance {
                Variance::Covariant => 0,
                Variance::Contravariant => 1,
                Variance::Invariant => 2,
            },
            contextual,
        );
        if !self.aliases.borrow_mut().constraints.insert(key) {
            return true;
        }
        self.constraints(
            self.expanded_alias_instance(expected),
            self.expanded_alias_instance(actual),
            variance,
            contextual,
            bounds,
        );
        self.aliases.borrow_mut().constraints.remove(&key);
        true
    }

    pub(super) fn recursive_union_constraints(
        &self,
        expected: &[Type],
        actual: Type,
        variance: Variance,
        contextual: bool,
        bounds: &mut BTreeMap<Type, Bounds>,
    ) -> bool {
        if !expected
            .iter()
            .any(|&ty| self.has_recursive_alias_reference(ty))
        {
            return false;
        }
        let Some(variable) = expected.iter().copied().find(|&ty| {
            self.node(ty)
                .is_some_and(|node| matches!(node.as_ref(), Node::Variable(_)))
        }) else {
            return false;
        };
        if contextual
            && self.node(actual).is_some_and(|node| {
                matches!(node.as_ref(), Node::Union(_))
                    || self.has_recursive_alias_reference(actual)
            })
        {
            self.constraints(variable, actual, variance, contextual, bounds);
            return true;
        }
        if self
            .node(actual)
            .is_some_and(|node| matches!(node.as_ref(), Node::Union(_)))
        {
            return false;
        }
        let structural = expected.iter().copied().find(|&ty| {
            self.constraint_shape_matches(ty, actual)
                && self.node(ty).is_some_and(|node| {
                    matches!(
                        node.as_ref(),
                        Node::VariadicTuple(_)
                            | Node::Container(
                                Container::Sequence | Container::Iterable | Container::Mapping,
                                _
                            )
                    ) || (matches!(node.as_ref(), Node::Container(_, _))
                        && self.empty_container_type(actual))
                })
        });
        self.constraints(
            structural.unwrap_or(variable),
            actual,
            variance,
            contextual,
            bounds,
        );
        true
    }

    fn empty_container_type(&self, ty: Type) -> bool {
        self.node(ty).is_some_and(|node| matches!(node.as_ref(), Node::Container(_, items) if items.iter().all(|&ty| ty == Type::Never)))
    }

    fn has_recursive_alias_reference(&self, ty: Type) -> bool {
        let Some(node) = self.node(ty) else {
            return false;
        };
        match node.as_ref() {
            Node::AliasInstance(id, _) => self.recursive_alias(*id),
            Node::Container(_, items) | Node::Tuple(items) | Node::Union(items) => items
                .iter()
                .any(|&ty| self.has_recursive_alias_reference(ty)),
            Node::VariadicTuple(item) => self.has_recursive_alias_reference(*item),
            _ => false,
        }
    }
}
