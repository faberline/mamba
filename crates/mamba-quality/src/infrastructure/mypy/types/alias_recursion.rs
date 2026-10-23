//! Coinductive comparison closes only an already active recursive type pair.
use super::*;

impl Context {
    pub(in crate::infrastructure::mypy) fn recursive_alias(&self, id: u32) -> bool {
        self.alias(id).target.is_some_and(|target| {
            self.alias_reaches(target, id, &mut std::collections::BTreeSet::new())
        })
    }

    fn alias_reaches(
        &self,
        ty: Type,
        id: u32,
        visited: &mut std::collections::BTreeSet<u32>,
    ) -> bool {
        let Some(node) = self.node(ty) else {
            return false;
        };
        match node.as_ref() {
            Node::AliasInstance(target, arguments) => {
                *target == id
                    || arguments
                        .iter()
                        .any(|&ty| self.alias_reaches(ty, id, visited))
                    || (visited.insert(*target)
                        && self
                            .alias(*target)
                            .target
                            .is_some_and(|ty| self.alias_reaches(ty, id, visited)))
            }
            Node::Container(_, items)
            | Node::Tuple(items)
            | Node::Union(items)
            | Node::AppliedClass {
                arguments: items, ..
            } => items.iter().any(|&ty| self.alias_reaches(ty, id, visited)),
            Node::VariadicTuple(item) => self.alias_reaches(*item, id, visited),
            _ => false,
        }
    }

    pub(in crate::infrastructure::mypy) fn unguarded_alias_cycle(&self, id: u32) -> bool {
        self.alias(id).target.is_some_and(|ty| {
            self.unguarded_alias_reaches(ty, id, &mut std::collections::BTreeSet::new())
        })
    }

    fn unguarded_alias_reaches(
        &self,
        ty: Type,
        id: u32,
        visited: &mut std::collections::BTreeSet<u32>,
    ) -> bool {
        let Some(node) = self.node(ty) else {
            return false;
        };
        match node.as_ref() {
            Node::AliasInstance(target, _) => {
                *target == id
                    || (visited.insert(*target)
                        && self
                            .alias(*target)
                            .target
                            .is_some_and(|ty| self.unguarded_alias_reaches(ty, id, visited)))
            }
            Node::Union(items) => items
                .iter()
                .any(|&ty| self.unguarded_alias_reaches(ty, id, visited)),
            _ => false,
        }
    }

    pub(super) fn recursive_alias_accepts(&self, expected: Type, actual: Type) -> bool {
        if expected == actual
            || expected.is_any()
            || actual.is_any()
            || expected == Type::Object
            || actual == Type::Never
        {
            return true;
        }
        let pair = (expected, actual);
        if !self.aliases.borrow_mut().comparisons.insert(pair) {
            return true;
        }
        let result = self.accepts(
            self.expanded_alias_instance(expected),
            self.expanded_alias_instance(actual),
        );
        self.aliases.borrow_mut().comparisons.remove(&pair);
        result
    }
}
