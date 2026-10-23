//! Standard collection declarations retain the pinned type checker definitions.
use super::{
    classes::{Abstractness, Method, MethodKind},
    declarations::Reader,
    types::{Context, Type},
};
use mamba_language::application::quality_syntax::PythonTarget;
use serde_json::Value;
use std::{collections::BTreeMap, sync::Arc, sync::OnceLock};

fn slot(target: PythonTarget) -> usize {
    match target {
        PythonTarget::Py312 => 0,
        PythonTarget::Py313 => 1,
        PythonTarget::Py314 => 2,
    }
}

fn data(target: PythonTarget) -> &'static Value {
    static DATA: OnceLock<Value> = OnceLock::new();
    let data = DATA.get_or_init(|| {
        serde_json::from_str(include_str!("../../../assets/stdlib/collections.json"))
            .expect("locked standard collection declarations")
    });
    &data["targets"][match target {
        PythonTarget::Py312 => "3.12",
        PythonTarget::Py313 => "3.13",
        PythonTarget::Py314 => "3.14",
    }]
}

impl Context {
    pub(in crate::infrastructure::mypy) fn missing_module_attribute(
        &self,
        module: &str,
        member: &str,
        target: PythonTarget,
    ) -> String {
        format!(
            "Module has no attribute \"{member}\"{}",
            self.standard_module_suggestion(module, member, target)
        )
    }

    pub(in crate::infrastructure::mypy) fn standard_module_suggestion(
        &self,
        module: &str,
        member: &str,
        target: PythonTarget,
    ) -> String {
        let Some(symbols) = data(target)["symbols"][module].as_object() else {
            return String::new();
        };
        let names = symbols.keys().map(String::as_str).collect();
        let matches = super::signatures::suggestions::best_matches(member, &names);
        let quoted: Vec<_> = matches.iter().map(|name| format!("\"{name}\"")).collect();
        let suggestion = match quoted.as_slice() {
            [] => return String::new(),
            [one] => one.clone(),
            [first, second] => format!("{first} or {second}"),
            many => format!(
                "{}, or {}",
                many[..many.len() - 1].join(", "),
                many.last().unwrap()
            ),
        };
        format!("; maybe {suggestion}?")
    }
    pub(in crate::infrastructure::mypy) fn typing_import_hint(
        &self,
        message: &str,
        target: PythonTarget,
    ) -> Option<String> {
        let name = message
            .strip_prefix("Module \"typing\" has no attribute \"")
            .or_else(|| {
                message.strip_prefix("Module \"typing\" does not explicitly export attribute \"")
            })?
            .strip_suffix('"')?;
        data(target)["symbols"]["typing_extensions"].get(name)?;
        Some(format!("Use `from typing_extensions import {name}` instead\nSee https://mypy.readthedocs.io/en/stable/runtime_troubles.html#using-new-additions-to-the-typing-module"))
    }
    pub(in crate::infrastructure::mypy) fn standard_collection_symbol(
        &self,
        module: &str,
        member: &str,
        target: PythonTarget,
    ) -> Option<bool> {
        let symbol = data(target)["symbols"][module].get(member)?;
        Some(
            !symbol["hidden"]
                .as_bool()
                .expect("standard symbol visibility"),
        )
    }

    pub(in crate::infrastructure::mypy) fn standard_collection(
        &self,
        module: &str,
        member: &str,
        target: PythonTarget,
    ) -> Option<Type> {
        let data = data(target);
        let export = data["exports"][module].get(member)?;
        let fullname = export["fullname"].as_str()?;
        self.load_standard_collections(data, target);
        let name = fullname.rsplit('.').next()?;
        Some(self.class(self.declare_class(fullname, name, 0)).object)
    }

    fn load_standard_collections(&self, data: &Value, target: PythonTarget) {
        if self.classes.borrow().standard_collections[slot(target)] {
            return;
        }
        self.classes.borrow_mut().standard_collections[slot(target)] = true;
        let values = data["classes"].as_object().expect("standard classes");
        for (name, value) in values {
            self.declare_class(value["fullname"].as_str().expect("class identity"), name, 0);
        }
        let mut reader = Reader {
            context: self,
            aliases: BTreeMap::new(),
            original_aliases: BTreeMap::new(),
            variables: BTreeMap::new(),
            aliases_ready: true,
        };
        for (name, value) in values {
            let fullname = value["fullname"].as_str().expect("class identity");
            let id = self.declare_class(fullname, name, 0);
            let mut class = (*self.class(id)).clone();
            class.generic = value["generic"]
                .as_array()
                .expect("class parameters")
                .iter()
                .map(|value| reader.ty(value))
                .collect();
            let bases: Vec<_> = value["bases"]
                .as_array()
                .expect("class bases")
                .iter()
                .map(|value| reader.ty(value))
                .collect();
            class.base_types = bases
                .iter()
                .copied()
                .filter(|&base| self.class_id(base).is_some())
                .collect();
            class.bases = class
                .base_types
                .iter()
                .filter_map(|&base| self.class_id(base))
                .collect();
            class.scalar_bases = bases
                .into_iter()
                .filter(|&base| self.class_id(base).is_none())
                .collect();
            class.protocol = value["protocol"].as_bool().expect("protocol status");
            class.runtime_protocol = value["runtime_protocol"]
                .as_bool()
                .expect("runtime protocol status");
            for (name, method) in value["methods"].as_object().expect("class methods") {
                let mut signature = if method["node_kind"] == "Decorator" {
                    reader.decorated_signature(&method["type"])
                } else {
                    reader.signature(&method["type"])
                };
                signature.name = Some(name.clone());
                signature.native_symbol = None;
                signature.owner = Some(class.name.clone());
                signature.definition_module = Some("typing".into());
                if let Some(receiver) = Arc::make_mut(&mut signature.parameters).first_mut() {
                    receiver.name = "self".into();
                }
                let kind = if method["property"] == true {
                    MethodKind::Property
                } else if method["class_method"] == true {
                    MethodKind::Class
                } else if method["static_method"] == true {
                    MethodKind::Static
                } else {
                    MethodKind::Instance
                };
                class.methods.insert(
                    name.clone(),
                    Method {
                        signature: Arc::new(signature),
                        kind,
                        setter: None,
                        final_method: false,
                        abstractness: match method["abstract_status"]
                            .as_u64()
                            .expect("method abstraction")
                        {
                            0 => Abstractness::Concrete,
                            1 => Abstractness::Explicit,
                            2 => Abstractness::Implicit,
                            code => panic!("unsupported standard abstract status: {code}"),
                        },
                    },
                );
            }
            self.replace_class(id, class);
        }
    }
}
