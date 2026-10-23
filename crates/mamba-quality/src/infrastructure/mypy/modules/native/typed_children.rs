//! Qualified semantic-platform child declarations stay separate from parent exports.
use super::{Export, Lookup};
#[cfg(test)]
mod tests;
use crate::infrastructure::mypy::{declarations::Reader, signatures::Signature, types::Context};
use mamba_language::application::quality_syntax::PythonTarget;
use serde::Deserialize;
use serde_json::Value;
use std::{
    collections::BTreeMap,
    sync::{Arc, OnceLock},
};

#[derive(Deserialize)]
struct Data {
    platforms: BTreeMap<String, BTreeMap<String, Child>>,
}
#[derive(Deserialize)]
struct Child {
    module: String,
    symbols: BTreeMap<String, Symbol>,
    functions: BTreeMap<String, Value>,
    aliases: BTreeMap<String, Value>,
}
#[derive(Deserialize)]
struct Symbol {
    explicit: bool,
}

fn child(target: PythonTarget, module: &str) -> Option<&'static Child> {
    let platform = if cfg!(target_os = "macos") {
        "darwin"
    } else if cfg!(target_os = "linux") {
        "linux"
    } else {
        return None;
    };
    static DATA: OnceLock<Data> = OnceLock::new();
    let data = DATA.get_or_init(|| {
        serde_json::from_str(include_str!(
            "../../../../../assets/stdlib/typed-child-modules.json"
        ))
        .expect("qualified child module declarations")
    });
    let version = match target {
        PythonTarget::Py312 => "3.12",
        PythonTarget::Py313 => "3.13",
        PythonTarget::Py314 => "3.14",
    };
    data.platforms
        .get(platform)?
        .get(version)
        .filter(|entry| entry.module == module)
}

fn signature(context: &Context, target: PythonTarget, entry: &Child, value: &Value) -> Signature {
    let mut reader = Reader {
        context,
        aliases: entry
            .aliases
            .keys()
            .map(|name| {
                let short = name.rsplit('.').next().expect("qualified alias");
                (
                    name.clone(),
                    context.declare_alias(
                        &format!("<typed-child:{target:?}>:{name}"),
                        short,
                        0,
                        false,
                        false,
                    ),
                )
            })
            .collect(),
        original_aliases: entry.aliases.clone(),
        variables: BTreeMap::new(),
        aliases_ready: false,
    };
    for (name, value) in &entry.aliases {
        let ty = reader.alias_target(name, value);
        context.set_embedded_alias_target(reader.aliases[name], ty);
    }
    reader.aliases_ready = true;
    let mut signature = reader.signature(value);
    signature.name = value["name"].as_str().map(String::from);
    signature.native_symbol = None;
    signature.definition_module = Some(entry.module.clone());
    signature.constructor = false;
    signature
}

impl Lookup {
    fn typed_child_shadowed(&self, module: &str) -> bool {
        let project = self.modules.info[self.current].0;
        let shadowed = |name: &str| {
            self.modules
                .shadow_namespaces
                .contains(&(project, name.into()))
        };
        shadowed(module)
            || module
                .match_indices('.')
                .any(|(end, _)| shadowed(&module[..end]))
    }

    pub(in crate::infrastructure::mypy) fn typed_child_module_exists(&self, module: &str) -> bool {
        !self.typed_child_shadowed(module)
            && child(
                self.modules.context.module_target(&self.namespace()),
                module,
            )
            .is_some()
    }

    pub(in crate::infrastructure::mypy) fn typed_child_member(
        &self,
        module: &str,
        member: &str,
    ) -> Option<Export> {
        if self.typed_child_shadowed(module) {
            return None;
        }
        let target = self.modules.context.module_target(&self.namespace());
        let entry = child(target, module)?;
        let symbol = entry.symbols.get(member)?;
        // Only the two captured typed callables are implemented in this slice.
        // Known but unimplemented symbols do not become dynamic Any exports.
        let value = entry.functions.get(member)?;
        let context = &self.modules.context;
        let mut callable = signature(context, target, entry, value);
        if value[".class"] == "Overloaded" {
            callable.overloads = value["items"]
                .as_array()
                .expect("locked overloads")
                .iter()
                .map(|item| Arc::new(signature(context, target, entry, item)))
                .collect::<Vec<_>>()
                .into();
        }
        let callable = Arc::new(callable);
        Some(Export {
            annotation_module: false,
            ty: Some(context.callable_type(callable.clone())),
            callable: Some(callable),
            module: None,
            explicit: symbol.explicit,
            module_hidden: None,
            alias: None,
        })
    }
}
