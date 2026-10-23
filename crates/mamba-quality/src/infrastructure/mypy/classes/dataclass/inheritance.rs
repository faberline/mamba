//! Transform inheritance retains declaration identity and direct metaclass neutrality.
use super::transforms::Spec;
use super::*;

impl Context {
    pub(super) fn dataclass_inherited_transform(
        &self,
        class: &Class,
        statement: &StmtClassDef,
        variables: &Variables,
    ) -> Option<Spec> {
        let id = self.class_id(class.instance)?;
        let direct = statement
            .arguments
            .as_ref()
            .and_then(|arguments| {
                arguments.keywords.iter().find(|keyword| {
                    keyword
                        .arg
                        .as_ref()
                        .is_some_and(|name| name.as_str() == "metaclass")
                })
            })
            .and_then(|keyword| self.annotation_with_variables(&keyword.value, variables))
            .and_then(|ty| self.class_id(ty));
        let metaclass = direct.or_else(|| {
            class.bases.iter().find_map(|id| {
                self.classes
                    .borrow()
                    .dataclass_transforms
                    .metaclasses
                    .get(id)
                    .copied()
            })
        });
        {
            let mut classes = self.classes.borrow_mut();
            let registry = &mut classes.dataclass_transforms;
            registry.metaclasses.remove(&id);
            registry.direct_metaclasses.remove(&id);
            if let Some(meta) = metaclass {
                registry.metaclasses.insert(id, meta);
            }
            if let Some(meta) = direct {
                registry.direct_metaclasses.insert(id, meta);
            }
        }
        let index = self
            .class_lineage(id)
            .into_iter()
            .skip(1)
            .find_map(|id| {
                self.classes
                    .borrow()
                    .dataclass_transforms
                    .class_specs
                    .get(&id)
                    .copied()
            })
            .or_else(|| {
                metaclass.and_then(|id| {
                    self.classes
                        .borrow()
                        .dataclass_transforms
                        .class_specs
                        .get(&id)
                        .copied()
                })
            })?;
        self.classes
            .borrow()
            .dataclass_transforms
            .specs
            .get(index)
            .cloned()
    }

    pub(in crate::infrastructure::mypy) fn recorded_metaclass(&self, id: u32) -> Option<u32> {
        self.classes
            .borrow()
            .dataclass_transforms
            .metaclasses
            .get(&id)
            .copied()
    }

    pub(in crate::infrastructure::mypy) fn dataclass_direct_transform_metaclass(
        &self,
        id: u32,
    ) -> bool {
        let classes = self.classes.borrow();
        let registry = &classes.dataclass_transforms;
        registry
            .direct_metaclasses
            .get(&id)
            .is_some_and(|id| registry.class_specs.contains_key(id))
    }

    pub(in crate::infrastructure::mypy) fn dataclass_transform_metaclass(&self, id: u32) -> bool {
        let classes = self.classes.borrow();
        let registry = &classes.dataclass_transforms;
        registry
            .metaclasses
            .get(&id)
            .is_some_and(|id| registry.class_specs.contains_key(id))
    }
}
