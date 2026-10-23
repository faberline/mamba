//! Build-time attribute stores retain their inferred class and declaring frame.
use super::{
    callables::Target,
    classes::{AttributeDefinition, Class},
    values::Value,
};
use mamba_language::application::quality_syntax::python_ast::ExprAttribute;
use ruff_text_size::{Ranged, TextRange};
use std::{collections::BTreeMap, rc::Rc};

#[derive(Clone)]
struct Store {
    module: String,
    name: String,
    instance: bool,
    target: Target,
    range: TextRange,
    frame: String,
    setter: bool,
    annotation_only: bool,
    augmented: bool,
}

#[derive(Clone)]
pub(super) struct Frame {
    pub module: String,
    pub name: String,
    pub setter: bool,
    pub annotation_only: bool,
    pub augmented: bool,
}

#[derive(Clone, Default)]
pub(super) struct Stores(BTreeMap<(String, u32), Vec<Store>>);

impl Stores {
    pub(super) fn record(
        &mut self,
        class: &Class,
        attribute: &ExprAttribute,
        instance: bool,
        value: Value,
        frame: &Frame,
    ) {
        self.0
            .entry(class.identity.clone())
            .or_default()
            .push(Store {
                module: frame.module.clone(),
                name: attribute.attr.to_string(),
                instance,
                target: Target::Value(Rc::new(value)),
                range: attribute.range(),
                frame: frame.name.clone(),
                setter: frame.setter,
                annotation_only: frame.annotation_only,
                augmented: frame.augmented,
            });
    }

    pub(super) fn augmented(&self, class: &Class, range: TextRange) -> bool {
        self.0.get(&class.identity).is_some_and(|stores| {
            stores
                .iter()
                .any(|store| store.range == range && store.augmented)
        })
    }

    pub(super) fn apply(&self, class: &mut Class) {
        let Some(stores) = self.0.get(&class.identity) else {
            return;
        };
        for store in stores {
            if store.instance {
                if !store.augmented {
                    class
                        .attributes
                        .insert(store.name.clone(), store.target.clone());
                }
                if !class.attribute_definitions.iter().any(|value| {
                    value.range == store.range
                        && value.module.as_deref().unwrap_or(&class.identity.0) == store.module
                }) {
                    class.attribute_definitions.push(AttributeDefinition {
                        module: Some(store.module.clone()),
                        name: store.name.clone(),
                        method: store.frame.clone(),
                        range: store.range,
                        setter: store.setter,
                    });
                }
            } else {
                let already_has_value = class.declared_members.contains(&store.name)
                    && !class.annotation_only.contains(&store.name);
                if !store.augmented {
                    class
                        .members
                        .insert(store.name.clone(), store.target.clone());
                    class
                        .first_members
                        .entry(store.name.clone())
                        .or_insert_with(|| store.target.clone());
                }
                class.declared_members.insert(store.name.clone());
                if !store.annotation_only {
                    class.annotation_only.remove(&store.name);
                } else if !already_has_value {
                    class.annotation_only.insert(store.name.clone());
                }
            }
        }
        class.attribute_definitions.sort_by_key(|value| {
            (
                value
                    .module
                    .as_deref()
                    .is_some_and(|module| module != class.identity.0),
                value.range.start(),
            )
        });
    }
}
