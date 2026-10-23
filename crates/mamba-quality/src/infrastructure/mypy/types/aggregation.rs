//! Aggregation functions use pinned overloads and structural helper declarations.
use super::*;
use crate::infrastructure::mypy::{declarations::Reader, signatures::Signature};
use mamba_language::application::quality_syntax::PythonTarget;
use serde_json::Value;
use std::sync::{Arc, OnceLock};

pub(super) fn data(target: PythonTarget) -> &'static Value {
    static DATA: OnceLock<Value> = OnceLock::new();
    let key = match target {
        PythonTarget::Py312 => "3.12",
        PythonTarget::Py313 => "3.13",
        PythonTarget::Py314 => "3.14",
    };
    &DATA.get_or_init(|| {
        serde_json::from_str(include_str!("../../../../assets/stdlib/aggregation.json"))
            .expect("locked aggregation declarations")
    })["targets"][key]
}

impl Context {
    pub(in crate::infrastructure::mypy) fn aggregation_function(
        &self,
        name: &str,
        target: PythonTarget,
    ) -> Option<Arc<Signature>> {
        let data = data(target);
        let constructor = name == "zip";
        let value = if constructor {
            data["constructors"].get(name)?
        } else {
            data["functions"].get(name)?
        };
        let key = format!("{target:?}:aggregation:{name}");
        if let Some(signature) = self.classes.borrow().collection_functions.get(&key) {
            return Some(signature.clone());
        }
        self.standard_collection("typing", "Iterable", target)?;
        let aliases = data["aliases"].as_object().expect("aggregation aliases");
        let mut reader = Reader {
            context: self,
            original_aliases: aliases
                .iter()
                .map(|(name, value)| (name.clone(), value.clone()))
                .collect(),
            aliases_ready: false,
            variables: BTreeMap::new(),
            aliases: aliases
                .keys()
                .map(|name| {
                    let id = self.declare_alias(
                        &format!("<aggregation:{target:?}>:{name}"),
                        name.rsplit('.').next().expect("alias identity"),
                        0,
                        false,
                        false,
                    );
                    (name.clone(), id)
                })
                .collect(),
        };
        for (name, value) in aliases {
            let ty = reader.alias_target(name, value);
            self.set_embedded_alias_target(reader.aliases[name], ty);
        }
        reader.aliases_ready = true;
        let slot = match target {
            PythonTarget::Py312 => 0,
            PythonTarget::Py313 => 1,
            PythonTarget::Py314 => 2,
        };
        if !self.classes.borrow().aggregation_declarations[slot] {
            self.classes.borrow_mut().aggregation_declarations[slot] = true;
            self.load_collection_declarations(data, &mut reader);
        }
        let mut signature = reader.signature(value);
        normalize(&mut signature, name, constructor);
        let signature = Arc::new(signature);
        self.classes
            .borrow_mut()
            .collection_functions
            .insert(key, signature.clone());
        Some(signature)
    }
}

fn normalize(signature: &mut Signature, name: &str, constructor: bool) {
    signature.name = Some(name.into());
    signature.constructor = constructor;
    signature.native_symbol = match name {
        "sorted" => Some("builtins.sorted"),
        "sum" => Some("builtins.sum"),
        "zip" => Some("builtins.zip"),
        _ => None,
    };
    signature.definition_module = Some("builtins".into());
    for variant in Arc::make_mut(&mut signature.overloads) {
        normalize(Arc::make_mut(variant), name, constructor);
    }
}
