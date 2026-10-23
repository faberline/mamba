//! Disallowed names and generic parameter shapes follow the locked original profile.
use super::Checker;
use mamba_language::application::quality_syntax::python_ast::{TypeParam, TypeParams};
use ruff_text_size::{Ranged, TextRange};
use std::{collections::BTreeSet, sync::OnceLock};
pub(super) fn generic_name(name: &str, kind: &str) -> bool {
    if name.starts_with("___") {
        return false;
    }
    let mut name = name.trim_start_matches('_');
    if kind == "typevar" {
        name = name
            .strip_suffix("_contra")
            .or_else(|| name.strip_suffix("_co"))
            .unwrap_or(name);
        if name.starts_with('T') && name.as_bytes().get(1).is_some_and(u8::is_ascii_uppercase) {
            return false;
        }
    }
    if name.ends_with("Type") {
        return false;
    }
    static T: OnceLock<regex::Regex> = OnceLock::new();
    static P: OnceLock<regex::Regex> = OnceLock::new();
    static TS: OnceLock<regex::Regex> = OnceLock::new();
    let (value, pattern) = match kind {
        "paramspec" => (&P, r"^(?:[A-Z]+|(?:[A-Z]+[a-z0-9]+)+(?:P)?)$"),
        "typevartuple" => (&TS, r"^(?:[A-Z]+|(?:[A-Z]+[a-z0-9]+)+(?:Ts)?)$"),
        _ => (&T, r"^(?:[A-Z]+|(?:[A-Z]+[a-z0-9]+)+(?:T)?)$"),
    };
    value
        .get_or_init(|| regex::Regex::new(pattern).expect("locked generic naming grammar"))
        .is_match(name)
}
impl Checker<'_> {
    pub(super) fn check_bad_name(&mut self, name: &str, range: TextRange) -> bool {
        if super::defaults::fixed().bad(name) {
            self.emit(
                "C0104",
                "disallowed-name",
                &format!("Disallowed name \"{name}\""),
                range,
            );
            true
        } else {
            false
        }
    }
    pub(super) fn check_generic_name(&mut self, name: &str, kind: &str, range: TextRange) -> bool {
        if self.check_bad_name(name, range) || super::defaults::fixed().good(name) {
            return false;
        }
        if !generic_name(name, kind) {
            let label = match kind {
                "paramspec" => "Parameter specification variable",
                "typevartuple" => "Type variable tuple",
                _ => "Type variable",
            };
            self.emit(
                "C0103",
                "invalid-name",
                &format!("{label} name \"{name}\" doesn't conform to predefined naming style"),
                range,
            );
        }
        true
    }
    pub(super) fn check_generic_shadowing(&mut self, parameters: Option<&TypeParams>) {
        for parameter in parameters.into_iter().flat_map(|value| &value.type_params) {
            let name = match parameter {
                TypeParam::TypeVar(value) => &value.name,
                TypeParam::TypeVarTuple(value) => &value.name,
                TypeParam::ParamSpec(value) => &value.name,
            };
            let Some(outer) = self.scopes.global(name.as_str()) else {
                continue;
            };
            if super::defaults::fixed().ignored(name.as_str(), false) {
                continue;
            }
            let line = ruff_source_file::LineIndex::from_source_text(&self.source.text)
                .line_column(outer.range.start(), &self.source.text)
                .line
                .get();
            self.emit(
                "W0621",
                "redefined-outer-name",
                &format!("Redefining name '{}' from outer scope (line {line})", name),
                parameter.range(),
            );
        }
    }
    pub(super) fn enter_generic_names(&mut self, parameters: Option<&TypeParams>) {
        let names: BTreeSet<_> = parameters
            .into_iter()
            .flat_map(|value| &value.type_params)
            .map(|value| match value {
                TypeParam::TypeVar(value) => value.name.to_string(),
                TypeParam::TypeVarTuple(value) => value.name.to_string(),
                TypeParam::ParamSpec(value) => value.name.to_string(),
            })
            .collect();
        self.generic_names.push(names);
    }
    pub(super) fn check_generic_parameter(&mut self, parameter: &TypeParam) {
        let (name, kind) = match parameter {
            TypeParam::TypeVar(value) => (&value.name, "typevar"),
            TypeParam::TypeVarTuple(value) => (&value.name, "typevartuple"),
            TypeParam::ParamSpec(value) => (&value.name, "paramspec"),
        };
        self.check_generic_name(name.as_str(), kind, parameter.range());
    }
    pub(super) fn generic_declared(&self, name: &str) -> bool {
        self.generic_names
            .iter()
            .rev()
            .any(|names| names.contains(name))
    }
}
