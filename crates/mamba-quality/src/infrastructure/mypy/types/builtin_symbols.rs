//! Missing scalar members and suggestions use the locked original symbol tables.
use super::*;
use mamba_language::application::quality_syntax::PythonTarget;
use serde::Deserialize;
use std::{collections::BTreeSet, sync::OnceLock};

#[derive(Deserialize)]
struct Tables {
    versions: BTreeMap<String, Target>,
}

#[derive(Deserialize)]
struct Target {
    symbols: BTreeMap<String, Symbols>,
    common_mistakes: BTreeMap<String, Vec<String>>,
}

#[derive(Deserialize)]
struct Symbols {
    members: BTreeMap<String, String>,
    own_members: BTreeSet<String>,
}

fn target(target: PythonTarget) -> &'static Target {
    static TABLES: OnceLock<Tables> = OnceLock::new();
    let key = match target {
        PythonTarget::Py312 => "3.12",
        PythonTarget::Py313 => "3.13",
        PythonTarget::Py314 => "3.14",
    };
    &TABLES
        .get_or_init(|| {
            serde_json::from_str(include_str!("../../../../assets/builtin-symbols.json"))
                .expect("locked builtin symbol tables")
        })
        .versions[key]
}

impl Context {
    pub(in crate::infrastructure::mypy) fn missing_builtin_attribute(
        &self,
        ty: Type,
        member: &str,
        python: PythonTarget,
    ) -> Option<String> {
        if self.is_class_value(ty) {
            return None;
        }
        let scalar = self.builtin_scalar(ty)?;
        let name = self.name(scalar);
        let table = target(python);
        let symbols = table.symbols.get(&name)?;
        if symbols.members.contains_key(member) {
            return None;
        }
        let alternatives: BTreeSet<_> = symbols.own_members.iter().map(String::as_str).collect();
        let mut matches: Vec<_> = table
            .common_mistakes
            .get(member)
            .into_iter()
            .flatten()
            .filter(|suggestion| alternatives.contains(suggestion.as_str()))
            .cloned()
            .collect();
        matches.extend(super::super::signatures::suggestions::best_matches(
            member,
            &alternatives,
        ));
        if member == "__aiter__" && matches == ["__iter__"] {
            matches.clear();
        }
        let extra = match member {
            "__iter__" => " (not iterable)",
            "__aiter__" => " (not async iterable)",
            _ => "",
        };
        let mut message = format!("\"{name}\" has no attribute \"{member}\"");
        if !matches.is_empty() {
            let quoted: Vec<_> = matches.iter().map(|name| format!("\"{name}\"")).collect();
            let suggestion = match quoted.as_slice() {
                [only] => only.clone(),
                [first, second] => format!("{first} or {second}"),
                many => format!(
                    "{}, or {}",
                    many[..many.len() - 1].join(", "),
                    many.last().unwrap()
                ),
            };
            message.push_str(&format!("; maybe {suggestion}?"));
        }
        message.push_str(extra);
        Some(message)
    }
}
