//! Locked structural file protocols share the normal protocol checker.
use crate::infrastructure::mypy::{
    classes::{Abstractness, Method, MethodKind},
    declarations::Reader,
    types::Context,
};
use serde_json::Value;
use std::{collections::BTreeMap, sync::Arc};

pub(super) fn register(
    context: &Context,
    declarations: &BTreeMap<String, Value>,
    reader: &mut Reader<'_>,
) {
    for (fullname, value) in declarations {
        let (module, short) = fullname.rsplit_once('.').expect("protocol identity");
        let id = context.declare_class(fullname, short, 0);
        let mut class = (*context.class(id)).clone();
        let methods = value["methods"].as_object().expect("protocol methods");
        if class.protocol && methods.keys().all(|name| class.methods.contains_key(name)) {
            let mut changed = false;
            for name in methods.keys() {
                let method = class.methods.get_mut(name).expect("protocol member");
                let signature = Arc::make_mut(&mut method.signature);
                if let Some(receiver) = Arc::make_mut(&mut signature.parameters).first_mut() {
                    if receiver.name.is_empty() {
                        receiver.name = "self".into();
                        changed = true;
                    }
                }
            }
            if changed {
                context.replace_class(id, class);
            }
            continue;
        }
        class.protocol = true;
        if let Some(bases) = value.get("bases") {
            class.base_types = bases
                .as_array()
                .expect("protocol bases")
                .iter()
                .map(|base| reader.ty(base))
                .collect();
            class.bases = class
                .base_types
                .iter()
                .map(|&base| context.class_id(base).expect("registered protocol base"))
                .collect();
        }
        class.generic = value["generic"]
            .as_array()
            .expect("protocol type variables")
            .iter()
            .map(|ty| reader.ty(ty))
            .collect();
        for (name, method) in methods {
            let mut signature = reader.signature(method);
            signature.name = Some(name.clone());
            if let Some(receiver) = Arc::make_mut(&mut signature.parameters).first_mut() {
                receiver.name = "self".into();
            }
            signature.owner = None;
            signature.native_symbol = None;
            signature.definition_module = Some(module.into());
            class.methods.insert(
                name.clone(),
                Method {
                    signature: Arc::new(signature),
                    kind: MethodKind::Instance,
                    setter: None,
                    final_method: false,
                    abstractness: Abstractness::Explicit,
                },
            );
        }
        context.replace_class(id, class);
    }
}
