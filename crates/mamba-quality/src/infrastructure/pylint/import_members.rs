//! Import-member categories are data from the three pinned Astroid runtimes.
use mamba_language::application::quality_syntax::PythonTarget;
use serde::Deserialize;
use std::{collections::BTreeMap, sync::OnceLock};
#[derive(Deserialize)]
struct Module {
    members: BTreeMap<String, String>,
}
#[derive(Deserialize)]
struct Data {
    modules: BTreeMap<String, Module>,
}
pub(super) fn kind(module: &str, name: &str, target: PythonTarget) -> Option<&'static str> {
    static DATA: OnceLock<Vec<Data>> = OnceLock::new();
    let values = DATA.get_or_init(|| {
        serde_json::from_str(include_str!("../../../assets/pylint-import-members.json"))
            .expect("locked import-member data")
    });
    let index = match target {
        PythonTarget::Py312 => 0,
        PythonTarget::Py313 => 1,
        PythonTarget::Py314 => 2,
    };
    values[index]
        .modules
        .get(module)?
        .members
        .get(name)
        .map(String::as_str)
}
