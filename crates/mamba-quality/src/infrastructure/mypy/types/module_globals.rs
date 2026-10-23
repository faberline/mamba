//! Implicit module values follow the pinned semantic analyzer's declarations.
use super::*;
use crate::infrastructure::mypy::declarations::Reader;
use mamba_language::application::quality_syntax::PythonTarget;
use serde::Deserialize;
use std::{collections::BTreeSet, sync::OnceLock};

#[derive(Deserialize)]
struct NameTables {
    targets: BTreeMap<String, NameTarget>,
}

#[derive(Deserialize)]
struct NameTarget {
    builtin_names: BTreeSet<String>,
    unimported_hints: Vec<String>,
    typing_any_value: serde_json::Value,
    builtin_functions: BTreeMap<String, serde_json::Value>,
}

fn data(target: PythonTarget) -> &'static NameTarget {
    static TABLES: OnceLock<NameTables> = OnceLock::new();
    &TABLES
        .get_or_init(|| {
            serde_json::from_str(include_str!(
                "../../../../assets/stdlib/name-resolution.json"
            ))
            .expect("locked semantic name tables")
        })
        .targets[match target {
        PythonTarget::Py312 => "3.12",
        PythonTarget::Py313 => "3.13",
        PythonTarget::Py314 => "3.14",
    }]
}

impl Context {
    pub(in crate::infrastructure::mypy) fn name_builtin_names(
        &self,
        target: PythonTarget,
    ) -> &'static BTreeSet<String> {
        &data(target).builtin_names
    }

    pub(in crate::infrastructure::mypy) fn unimported_name_hint(
        &self,
        name: &str,
        target: PythonTarget,
    ) -> Option<String> {
        let fullname = data(target).unimported_hints.iter().find(|fullname| {
            fullname
                .rsplit('.')
                .next()
                .is_some_and(|value| value.to_lowercase() == name.to_lowercase())
        })?;
        let (module, name) = fullname.rsplit_once('.')?;
        Some(format!("Did you forget to import it from \"{module}\"? (Suggestion: \"from {module} import {name}\")"))
    }

    pub(in crate::infrastructure::mypy) fn typing_any_value(&self, target: PythonTarget) -> Type {
        let mut reader = Reader {
            context: self,
            aliases: BTreeMap::new(),
            original_aliases: BTreeMap::new(),
            variables: BTreeMap::new(),
            aliases_ready: true,
        };
        let value = &data(target).typing_any_value;
        let signature = native_signature(
            &mut reader,
            value,
            "typing",
            value["fallback"] == "builtins.type",
        );
        self.callable_type(std::sync::Arc::new(signature))
    }

    pub(in crate::infrastructure::mypy) fn name_builtin_function(
        &self,
        name: &str,
        target: PythonTarget,
    ) -> Option<std::sync::Arc<crate::infrastructure::mypy::signatures::Signature>> {
        let value = data(target).builtin_functions.get(name)?;
        let mut reader = Reader {
            context: self,
            aliases: BTreeMap::new(),
            original_aliases: BTreeMap::new(),
            variables: BTreeMap::new(),
            aliases_ready: true,
        };
        Some(std::sync::Arc::new(native_signature(
            &mut reader,
            value,
            "builtins",
            false,
        )))
    }

    pub(in crate::infrastructure::mypy) fn module_global(
        &self,
        name: &str,
        is_package: bool,
    ) -> Option<Type> {
        match name {
            "__name__" | "__doc__" | "__file__" | "__package__" => Some(Type::Str),
            "__annotations__" => {
                Some(self.intern(Node::Container(Container::Dict, vec![Type::Str, Type::Any])))
            }
            "__path__" if is_package => {
                Some(self.intern(Node::Container(Container::List, vec![Type::Str])))
            }
            "__spec__" => Some(
                self.class(self.declare_class("_frozen_importlib.ModuleSpec", "ModuleSpec", 0))
                    .instance,
            ),
            _ => None,
        }
    }
}

fn native_signature(
    reader: &mut Reader<'_>,
    value: &serde_json::Value,
    module: &str,
    constructor: bool,
) -> crate::infrastructure::mypy::signatures::Signature {
    let mut signature = reader.signature(value);
    signature.native_symbol = None;
    signature.definition_module = Some(module.into());
    signature.constructor = constructor;
    if value[".class"] == "Overloaded" {
        signature.overloads = value["items"]
            .as_array()
            .expect("native overloads")
            .iter()
            .map(|value| std::sync::Arc::new(native_signature(reader, value, module, constructor)))
            .collect::<Vec<_>>()
            .into();
        signature.name = signature.overloads.last().unwrap().name.clone();
    } else {
        signature.name = value["name"].as_str().map(String::from);
    }
    signature
}
