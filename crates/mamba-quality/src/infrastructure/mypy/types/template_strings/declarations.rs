//! Class and function declarations share the standard Rust signature reader.
use super::*;
use crate::infrastructure::mypy::{
    classes::{Abstractness, Field, Method, MethodKind},
    declarations::Reader,
};
use std::collections::BTreeMap;

impl Context {
    pub(super) fn load_template_library(&self) {
        if self.classes.borrow().template_library {
            return;
        }
        self.classes.borrow_mut().template_library = true;
        let data = data();
        let classes = data["classes"].as_object().expect("template classes");
        for (name, value) in classes {
            self.declare_class(value["fullname"].as_str().expect("class identity"), name, 0);
        }
        let mut reader = Reader {
            context: self,
            aliases: BTreeMap::new(),
            original_aliases: BTreeMap::new(),
            variables: BTreeMap::new(),
            aliases_ready: true,
        };
        let object = self.builtin_class(Type::Object);
        let mut object_class = (*self.class(object)).clone();
        for (name, value) in data["object_methods"].as_object().expect("object methods") {
            let mut signature = reader.signature(value);
            signature.name = Some(name.clone());
            signature.owner = Some("object".into());
            signature.native_symbol = None;
            signature.definition_module = Some("builtins".into());
            object_class.methods.insert(
                name.clone(),
                Method {
                    signature: Arc::new(signature),
                    kind: MethodKind::Instance,
                    setter: None,
                    final_method: false,
                    abstractness: Abstractness::Concrete,
                },
            );
        }
        self.replace_class(object, object_class);
        for (name, value) in classes {
            let id = self.declare_class(value["fullname"].as_str().unwrap(), name, 0);
            let mut class = (*self.class(id)).clone();
            class.generic = value["generic"]
                .as_array()
                .unwrap()
                .iter()
                .map(|value| reader.ty(value))
                .collect();
            let bases: Vec<_> = value["bases"]
                .as_array()
                .unwrap()
                .iter()
                .map(|value| reader.ty(value))
                .map(|ty| {
                    if ty == Type::Object {
                        self.class(object).instance
                    } else {
                        ty
                    }
                })
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
            class.final_class = value["final"].as_bool().unwrap();
            class.protocol = value["protocol"].as_bool().unwrap();
            class.runtime_protocol = value["runtime_protocol"].as_bool().unwrap();
            class.fields.clear();
            for (name, field) in value["fields"].as_object().unwrap() {
                class.fields.insert(
                    name.clone(),
                    Field {
                        ty: Some(reader.ty(&field["type"])),
                        needs_annotation: false,
                        class_access: field["initialized_in_class"].as_bool().unwrap(),
                        readonly: false,
                        final_attribute: field["final"].as_bool().unwrap(),
                        class_var: field["class_var"].as_bool().unwrap(),
                        abstract_field: false,
                    },
                );
            }
            class.methods.clear();
            for (name, method) in value["methods"].as_object().unwrap() {
                let mut signature = if method["node_kind"] == "Decorator" {
                    reader.decorated_signature(&method["type"])
                } else {
                    reader.signature(&method["type"])
                };
                signature.name = Some(name.clone());
                signature.native_symbol = None;
                signature.owner = Some(class.name.clone());
                signature.definition_module = Some("string.templatelib".into());
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
                        final_method: method["final"].as_bool().unwrap(),
                        abstractness: match method["abstract_status"].as_u64().unwrap() {
                            0 => Abstractness::Concrete,
                            1 => Abstractness::Explicit,
                            2 => Abstractness::Implicit,
                            code => panic!("unsupported template abstraction: {code}"),
                        },
                    },
                );
            }
            self.replace_class(id, class);
        }
        for (name, value) in data["functions"].as_object().unwrap() {
            let mut signature = reader.signature(value);
            name_function(&mut signature, name);
            self.classes
                .borrow_mut()
                .template_functions
                .insert(name.clone(), Arc::new(signature));
        }
    }
}

fn name_function(signature: &mut super::super::super::signatures::Signature, name: &str) {
    signature.name = Some(name.into());
    signature.native_symbol = None;
    signature.definition_module = Some("string.templatelib".into());
    for variant in Arc::make_mut(&mut signature.overloads) {
        name_function(Arc::make_mut(variant), name);
    }
}
