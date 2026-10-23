//! Constructor helper protocols and result classes retain their original members.
use super::*;
use crate::infrastructure::mypy::{
    classes::{Abstractness, Method, MethodKind},
    declarations::Reader,
    signatures::Signature,
};
use mamba_language::application::quality_syntax::PythonTarget;
use serde_json::Value;
use std::sync::Arc;

impl Context {
    pub(super) fn load_constructor_declarations(
        &self,
        data: &Value,
        target: PythonTarget,
        reader: &mut Reader<'_>,
    ) {
        let slot = match target {
            PythonTarget::Py312 => 0,
            PythonTarget::Py313 => 1,
            PythonTarget::Py314 => 2,
        };
        if self.classes.borrow().collection_constructor_protocols[slot] {
            return;
        }
        self.classes.borrow_mut().collection_constructor_protocols[slot] = true;
        self.load_collection_declarations(data, reader);
    }

    pub(super) fn load_collection_declarations(&self, data: &Value, reader: &mut Reader<'_>) {
        for (protocol, declarations) in [(true, &data["protocols"]), (false, &data["classes"])] {
            for (fullname, value) in declarations.as_object().expect("constructor declarations") {
                let (module, short) = fullname.rsplit_once('.').expect("declaration identity");
                let id = self.declare_class(fullname, short, 0);
                let mut class = (*self.class(id)).clone();
                class.protocol = protocol;
                if let Some(final_class) = value["final"].as_bool() {
                    class.final_class = final_class;
                }
                class.generic = value["generic"]
                    .as_array()
                    .expect("declaration parameters")
                    .iter()
                    .map(|value| reader.ty(value))
                    .collect();
                class.base_types = value["bases"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|value| reader.ty(value))
                    .collect();
                class.bases = class
                    .base_types
                    .iter()
                    .filter_map(|&ty| self.class_id(ty))
                    .collect();
                for (name, value) in value["methods"].as_object().expect("declaration methods") {
                    let mut signature = reader.signature(value);
                    normalize_member(&mut signature, name, (!protocol).then_some(short), module);
                    class.methods.insert(
                        name.clone(),
                        Method {
                            signature: Arc::new(signature),
                            kind: match declarations[fullname]["kinds"][name].as_str() {
                                Some("property") => MethodKind::Property,
                                Some("class") => MethodKind::Class,
                                _ => MethodKind::Instance,
                            },
                            setter: None,
                            final_method: false,
                            abstractness: if protocol {
                                Abstractness::Explicit
                            } else {
                                Abstractness::Concrete
                            },
                        },
                    );
                }
                self.replace_class(id, class);
            }
        }
    }
}

fn normalize_member(signature: &mut Signature, name: &str, owner: Option<&str>, module: &str) {
    signature.name = Some(name.into());
    signature.owner = owner.map(Into::into);
    signature.native_symbol = None;
    signature.definition_module = Some(module.into());
    if let Some(first) = Arc::make_mut(&mut signature.parameters).first_mut() {
        first.name = "self".into();
    }
    for variant in Arc::make_mut(&mut signature.overloads) {
        normalize_member(Arc::make_mut(variant), name, owner, module);
    }
}
