//! Original deprecation tables are embedded separately for each Python target.
use mamba_language::application::quality_syntax::PythonTarget;
use serde::Deserialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::OnceLock,
};

#[derive(Deserialize)]
pub(super) struct Data {
    pub methods: BTreeSet<String>,
    pub arguments: BTreeMap<String, Vec<(Option<usize>, String)>>,
    pub classes: BTreeMap<String, BTreeSet<String>>,
    pub decorators: BTreeSet<String>,
    pub attributes: BTreeSet<String>,
}

pub(super) fn fixed(target: PythonTarget) -> &'static Data {
    static DATA: OnceLock<Vec<Data>> = OnceLock::new();
    let data = DATA.get_or_init(|| {
        serde_json::from_str(include_str!("../../../assets/pylint-deprecated.json"))
            .expect("locked Pylint deprecation data")
    });
    &data[match target {
        PythonTarget::Py312 => 0,
        PythonTarget::Py313 => 1,
        PythonTarget::Py314 => 2,
    }]
}
