//! Built-in member checks use names from the pinned Astroid runtimes.
use mamba_language::application::quality_syntax::PythonTarget;
use serde::Deserialize;
use std::{collections::BTreeMap, sync::OnceLock};

#[derive(Deserialize)]
struct Data {
    classes: BTreeMap<String, Vec<String>>,
}
pub(super) fn names(class: &str, target: PythonTarget) -> Option<&'static [String]> {
    static DATA: OnceLock<Vec<Data>> = OnceLock::new();
    let data = DATA.get_or_init(|| {
        serde_json::from_str(include_str!("../../../assets/pylint-builtin-members.json"))
            .expect("locked Astroid built-in member data")
    });
    let index = match target {
        PythonTarget::Py312 => 0,
        PythonTarget::Py313 => 1,
        PythonTarget::Py314 => 2,
    };
    data[index].classes.get(class).map(Vec::as_slice)
}
pub(super) fn exists(class: &str, name: &str, target: PythonTarget) -> bool {
    names(class, target).is_some_and(|names| names.iter().any(|value| value == name))
}
pub(super) fn hint(class: &str, name: &str, target: PythonTarget) -> String {
    names(class, target)
        .and_then(|names| names.iter().find(|candidate| one_edit(name, candidate)))
        .map(|candidate| format!("; maybe '{candidate}'?"))
        .unwrap_or_default()
}
pub(super) fn one_edit(left: &str, right: &str) -> bool {
    let left: Vec<_> = left.chars().collect();
    let right: Vec<_> = right.chars().collect();
    if left.len().abs_diff(right.len()) > 1 || left == right {
        return false;
    }
    let (mut i, mut j, mut edits) = (0, 0, 0);
    while i < left.len() && j < right.len() {
        if left[i] == right[j] {
            i += 1;
            j += 1;
            continue;
        }
        edits += 1;
        if edits > 1 {
            return false;
        }
        if left.len() >= right.len() {
            i += 1;
        }
        if right.len() >= left.len() {
            j += 1;
        }
    }
    edits + usize::from(i < left.len() || j < right.len()) == 1
}
