//! Source class scope names retain the pinned class and instance distinction.
use mamba_language::application::quality_syntax::PythonTarget;
use serde::Deserialize;
use std::{collections::BTreeMap, sync::OnceLock};

#[derive(Deserialize)]
struct Attribute {
    available: bool,
}
#[derive(Deserialize)]
struct Object {
    attributes: BTreeMap<String, Attribute>,
}
#[derive(Deserialize)]
struct Data {
    objects: BTreeMap<String, Object>,
}

pub(super) fn exists(name: &str, instance: bool, target: PythonTarget) -> bool {
    static DATA: OnceLock<Vec<Data>> = OnceLock::new();
    let data = DATA.get_or_init(|| {
        serde_json::from_str(include_str!("../../../../assets/pylint-class-members.json"))
            .expect("locked Astroid source class member data")
    });
    let index = match target {
        PythonTarget::Py312 => 0,
        PythonTarget::Py313 => 1,
        PythonTarget::Py314 => 2,
    };
    data[index].objects[if instance { "instance" } else { "class" }]
        .attributes
        .get(name)
        .is_some_and(|attribute| attribute.available)
}

impl super::Checker<'_> {
    pub(in super::super) fn source_class_scope_member(
        &self,
        class: &super::Class,
        name: &str,
        instance: bool,
    ) -> bool {
        class.identity.0 != "builtins" && exists(name, instance, self.modules.target())
    }
}
