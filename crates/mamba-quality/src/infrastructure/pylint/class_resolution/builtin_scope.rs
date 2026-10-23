//! Builtin class scope facts retain pinned target-version model data.
use mamba_language::application::quality_syntax::PythonTarget;
use serde::Deserialize;
use serde_json::Value;
use std::{collections::BTreeMap, sync::OnceLock};

#[derive(Deserialize)]
pub(super) struct Attribute {
    pub available: bool,
    pub values: Vec<Value>,
}
#[derive(Deserialize)]
pub(super) struct Class {
    pub bases: Vec<String>,
    pub mro_names: Vec<String>,
    pub attributes: BTreeMap<String, Attribute>,
}
#[derive(Deserialize)]
struct Data {
    classes: BTreeMap<String, Class>,
}

pub(super) fn class(name: &str, target: PythonTarget) -> Option<&'static Class> {
    static DATA: OnceLock<Vec<Data>> = OnceLock::new();
    let data = DATA.get_or_init(|| {
        serde_json::from_str(include_str!(
            "../../../../assets/pylint-builtin-class-scope.json"
        ))
        .expect("locked isolated Astroid builtin class scope data")
    });
    let index = match target {
        PythonTarget::Py312 => 0,
        PythonTarget::Py313 => 1,
        PythonTarget::Py314 => 2,
    };
    data[index].classes.get(name)
}
