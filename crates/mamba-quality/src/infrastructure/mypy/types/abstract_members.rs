//! Abstract declarations use the first member definition in the class MRO.
use super::super::classes::Abstractness;
use super::*;
use std::collections::BTreeSet;

impl Context {
    pub(in crate::infrastructure::mypy) fn abstract_members(
        &self,
        id: u32,
    ) -> BTreeMap<String, bool> {
        let mut seen = BTreeSet::new();
        let mut abstract_members = BTreeMap::new();
        for id in self.class_lineage(id) {
            let class = self.class(id);
            for (name, method) in &class.methods {
                if seen.insert(name.clone()) && method.abstractness != Abstractness::Concrete {
                    abstract_members.insert(
                        name.clone(),
                        method.abstractness == Abstractness::Implicit
                            && method
                                .signature
                                .returns
                                .is_some_and(|ty| self.accepts(ty, Type::None)),
                    );
                }
            }
            for (name, field) in &class.fields {
                if seen.insert(name.clone()) && field.abstract_field {
                    abstract_members.insert(name.clone(), false);
                }
            }
        }
        abstract_members
    }
}
