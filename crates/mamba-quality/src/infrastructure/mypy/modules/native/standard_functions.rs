//! Pinned original standard functions use native callable signatures.
use super::{Export, Lookup};
mod protocols;
use crate::infrastructure::mypy::{
    declarations::Reader,
    signatures::Signature,
    types::{Node, Type},
};
use mamba_language::application::quality_syntax::PythonTarget;
use serde::Deserialize;
use serde_json::Value;
use std::{
    collections::BTreeMap,
    sync::{Arc, OnceLock},
};

#[derive(Deserialize)]
struct Data {
    platforms: BTreeMap<String, BTreeMap<String, Target>>,
}

#[derive(Deserialize)]
struct Target {
    aliases: BTreeMap<String, Value>,
    functools: BTreeMap<String, Value>,
    json: BTreeMap<String, Value>,
    math: BTreeMap<String, Value>,
    protocols: BTreeMap<String, Value>,
}

fn data(target: PythonTarget) -> &'static Target {
    static DATA: OnceLock<Data> = OnceLock::new();
    let platform = if cfg!(target_os = "macos") {
        "darwin"
    } else {
        "linux"
    };
    let version = match target {
        PythonTarget::Py312 => "3.12",
        PythonTarget::Py313 => "3.13",
        PythonTarget::Py314 => "3.14",
    };
    &DATA
        .get_or_init(|| {
            serde_json::from_str(include_str!(
                "../../../../../assets/stdlib/standard-functions.json"
            ))
            .expect("locked standard function declarations")
        })
        .platforms[platform][version]
}

impl Lookup {
    pub(super) fn standard_protocol_member(&self, module: &str, member: &str) -> Option<Export> {
        let target = self.modules.context.module_target(&self.namespace());
        let declarations = data(target);
        let fullname = format!("{module}.{member}");
        declarations.protocols.get(&fullname)?;
        let old = self.standard_module_member(module, member)?;
        let context = &self.modules.context;
        let mut reader = Reader {
            context,
            aliases: BTreeMap::new(),
            original_aliases: BTreeMap::new(),
            variables: BTreeMap::new(),
            aliases_ready: true,
        };
        protocols::register(context, &declarations.protocols, &mut reader);
        let id = context.declare_class(&fullname, member, 0);
        let ty = context.class(id).object;
        Some(Export {
            annotation_module: false,
            ty: Some(ty),
            callable: context.class_constructor(ty),
            module: None,
            explicit: old.explicit,
            module_hidden: old.module_hidden,
            alias: Some(context.annotation_value(ty)),
        })
    }

    pub(in crate::infrastructure::mypy) fn standard_function_member(
        &self,
        module: &str,
        member: &str,
    ) -> Option<Export> {
        let target = self.modules.context.module_target(&self.namespace());
        let declarations = data(target);
        let value = match module {
            "functools" => declarations.functools.get(member)?,
            "json" => declarations.json.get(member)?,
            "math" => declarations.math.get(member)?,
            _ => return None,
        };
        let old = self.standard_module_member(module, member)?;
        let context = &self.modules.context;
        if module == "functools" {
            context
                .builtin_value_class_instance("functools._lru_cache_wrapper", target)
                .expect("locked cache wrapper declaration");
        }
        let mut reader = Reader {
            context,
            aliases: declarations
                .aliases
                .keys()
                .map(|name| {
                    let id = context.declare_alias(
                        &format!("<standard-functions:{target:?}>:{name}"),
                        name.rsplit('.')
                            .next()
                            .expect("standard function alias name"),
                        0,
                        false,
                        false,
                    );
                    (name.clone(), id)
                })
                .collect(),
            original_aliases: declarations.aliases.clone(),
            variables: BTreeMap::new(),
            aliases_ready: false,
        };
        for (name, value) in &declarations.aliases {
            let ty = reader.alias_target(name, value);
            context.set_embedded_alias_target(reader.aliases[name], ty);
        }
        reader.aliases_ready = true;
        protocols::register(context, &declarations.protocols, &mut reader);
        let mut signature = standard_signature(&mut reader, value, module);
        signature.name = value["name"]
            .as_str()
            .or_else(|| {
                value["items"]
                    .as_array()
                    .and_then(|items| items.first())
                    .and_then(|item| item["name"].as_str())
            })
            .map(String::from);
        signature.native_symbol = None;
        signature.definition_module = Some(module.into());
        signature.constructor = false;
        if value[".class"] == "Overloaded" {
            signature.overloads = value["items"]
                .as_array()
                .expect("locked function overloads")
                .iter()
                .map(|variant| Arc::new(standard_signature(&mut reader, variant, module)))
                .collect::<Vec<_>>()
                .into();
        }
        let signature = Arc::new(signature);
        Some(Export {
            annotation_module: false,
            ty: Some(context.callable_type(signature.clone())),
            callable: Some(signature),
            module: None,
            explicit: old.explicit,
            module_hidden: old.module_hidden,
            alias: None,
        })
    }
}

fn standard_signature(reader: &mut Reader<'_>, value: &Value, module: &str) -> Signature {
    let mut signature = reader.signature(value);
    if let Some(parameters) = value["arg_types"].as_array() {
        for (parameter, original) in Arc::make_mut(&mut signature.parameters)
            .iter_mut()
            .zip(parameters)
        {
            if uses_nested_alias(reader, original) {
                parameter.ty = Some(grouped_type(reader, original));
            }
        }
    }
    signature.name = value["name"].as_str().map(String::from);
    signature.native_symbol = None;
    signature.definition_module = Some(module.into());
    signature.constructor = false;
    signature
}

fn uses_nested_alias(reader: &Reader<'_>, value: &Value) -> bool {
    if value[".class"] == "TypeAliasType" {
        let name = value["type_ref"].as_str().expect("locked alias reference");
        if let Some(target) = reader.original_aliases.get(name) {
            return contains_alias(target);
        }
    }
    match value {
        Value::Array(items) => items.iter().any(|item| uses_nested_alias(reader, item)),
        Value::Object(items) => items.values().any(|item| uses_nested_alias(reader, item)),
        _ => false,
    }
}

fn contains_alias(value: &Value) -> bool {
    if value[".class"] == "TypeAliasType" {
        return true;
    }
    match value {
        Value::Array(items) => items.iter().any(contains_alias),
        Value::Object(items) => items.values().any(contains_alias),
        _ => false,
    }
}

fn grouped_type(reader: &mut Reader<'_>, value: &Value) -> Type {
    match value[".class"].as_str() {
        Some("UnionType") => {
            let members = value["items"]
                .as_array()
                .expect("locked union members")
                .iter()
                .map(|item| grouped_type(reader, item))
                .collect();
            reader.context.intern(Node::Union(members))
        }
        Some("TypeAliasType") => {
            let name = value["type_ref"].as_str().expect("locked alias reference");
            let target = reader.original_aliases[name].clone();
            grouped_type(reader, &target)
        }
        Some("Instance")
            if value["args"]
                .as_array()
                .is_some_and(|args| !args.is_empty()) =>
        {
            let base = reader.ty(value);
            let id = reader.context.class_id(base).expect("locked generic class");
            let arguments = value["args"]
                .as_array()
                .expect("locked class arguments")
                .iter()
                .map(|arg| grouped_type(reader, arg))
                .collect();
            reader.context.applied_class(id, arguments, false)
        }
        _ => reader.ty(value),
    }
}
