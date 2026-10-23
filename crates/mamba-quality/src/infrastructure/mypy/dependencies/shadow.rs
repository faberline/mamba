//! Core library names must retain their embedded type definitions.
use super::local::Catalog;
use crate::domain::{
    model::{QualityDiagnostic, QualityLocation, QualityTool},
    source::QualityWorkspace,
};
use std::collections::BTreeSet;

const CORE_MODULES: [&str; 11] = [
    "builtins",
    "typing",
    "types",
    "typing_extensions",
    "mypy_extensions",
    "_typeshed",
    "_collections_abc",
    "collections",
    "collections.abc",
    "sys",
    "abc",
];

pub(super) fn find(workspace: &QualityWorkspace) -> Option<QualityDiagnostic> {
    let mut catalog = Catalog::default();
    let mut errors = BTreeSet::new();
    for roots in super::local::roots(workspace) {
        for name in CORE_MODULES {
            let sources =
                super::local::sources_for_diagnostic(&roots, name, &mut catalog, &mut errors);
            let Some((path, _)) = sources.into_iter().find(|(_, module)| module == name) else {
                continue;
            };
            return Some(QualityDiagnostic {
                tool: QualityTool::Mypy,
                rule: String::new(),
                message: format!("This file shadows library module \"{name}\""),
                hint: Some(format!(
                    "A user-defined top-level module with name \"{name}\" is not supported"
                )),
                path,
                location: QualityLocation::unpositioned(),
                severity: "error".into(),
                fix: None,
            });
        }
    }
    None
}
