//! Original information objects use ordinary nominal fields and method binding.
use super::{class_properties::signature, field_types::Reader};
use crate::infrastructure::mypy::{
    classes::{Abstractness, Field, Method, MethodKind},
    types::Type,
};
use serde_json::Value;
use std::sync::Arc;

pub(super) fn populate(data: &Value, reader: &mut Reader<'_>) {
    for (name, value) in data["info_classes"]
        .as_object()
        .expect("information classes")
    {
        let fullname = value["fullname"].as_str().expect("information identity");
        let id = reader.context.declare_class(fullname, name, 0);
        if value["initializer_generated"] == true {
            let fields = value["members"]
                .as_object()
                .expect("information members")
                .iter()
                .filter(|(_, declaration)| {
                    declaration["kind"] == "field" && declaration["class_var"] != true
                })
                .map(|(name, _)| name.clone())
                .collect();
            reader
                .context
                .classes
                .borrow_mut()
                .embedded_implicit_fields
                .insert(id, fields);
        }
        if let Some(slots) = value["slots"].as_array() {
            reader
                .context
                .classes
                .borrow_mut()
                .embedded_class_slots
                .insert(
                    id,
                    slots
                        .iter()
                        .map(|slot| slot.as_str().expect("slot name").into())
                        .collect(),
                );
        }
        let mut class = (*reader.context.class(id)).clone();
        class.final_class = value["final"] == true;
        let mut initializer = signature(reader, &value["initializer"], "__init__", fullname);
        if value["initializer_generated"] == true {
            initializer.definition_module = None;
        }
        class.methods.insert(
            "__init__".into(),
            Method {
                signature: Arc::new(initializer),
                kind: MethodKind::Instance,
                setter: None,
                final_method: false,
                abstractness: Abstractness::Concrete,
            },
        );
        let original = signature(reader, &value["raw_initializer"], "__init__", fullname);
        reader
            .context
            .classes
            .borrow_mut()
            .embedded_constructor_reveals
            .insert(
                id,
                Arc::new(original.bound(1).constructor_for(name, class.instance)),
            );
        for base in value["bases"].as_array().expect("information bases") {
            let ty = reader.ty(base);
            if ty == Type::Object {
                class.scalar_bases.push(ty);
            } else if let Some(parent) = reader.context.class_id(ty) {
                class.bases.push(parent);
                class.base_types.push(ty);
            }
        }
        for (member, declaration) in value["members"].as_object().expect("information members") {
            if declaration["kind"] == "field" {
                class.fields.insert(
                    member.clone(),
                    Field {
                        ty: Some(reader.ty(&declaration["type"])),
                        needs_annotation: false,
                        class_access: true,
                        readonly: declaration["final"] == true,
                        final_attribute: declaration["final"] == true,
                        class_var: declaration["class_var"] == true,
                        abstract_field: false,
                    },
                );
                continue;
            }
            let mut signature = signature(reader, &declaration["type"], member, fullname);
            signature.owner = Some(class.name.clone());
            signature.definition_module = Some("pydantic.fields".into());
            let kind = if declaration["property"] == true {
                MethodKind::Property
            } else if declaration["class_method"] == true {
                MethodKind::Class
            } else if declaration["static_method"] == true {
                MethodKind::Static
            } else {
                MethodKind::Instance
            };
            class.methods.insert(
                member.clone(),
                Method {
                    signature: Arc::new(signature),
                    kind,
                    setter: None,
                    final_method: false,
                    abstractness: Abstractness::Concrete,
                },
            );
        }
        reader.context.replace_class(id, class);
    }
    for (name, value) in data["comparison_protocols"]
        .as_object()
        .expect("comparison protocols")
    {
        let fullname = value["fullname"].as_str().expect("protocol identity");
        let id = reader.context.declare_class(fullname, name, 0);
        let mut class = (*reader.context.class(id)).clone();
        class.protocol = value["protocol"] == true;
        let name = value["method"].as_str().expect("comparison member");
        class.methods.insert(
            name.into(),
            Method {
                signature: Arc::new(signature(reader, &value["type"], name, fullname)),
                kind: MethodKind::Instance,
                setter: None,
                final_method: false,
                abstractness: Abstractness::Implicit,
            },
        );
        reader.context.replace_class(id, class);
    }
    for (name, methods) in data["scalar_comparisons"]
        .as_object()
        .expect("scalar comparisons")
    {
        let ty = crate::infrastructure::mypy::types::Context::builtin_name(name)
            .expect("comparison scalar");
        let owner = format!("builtins.{}", reader.context.name(ty));
        for (name, value) in methods.as_object().expect("scalar comparison members") {
            let declaration = signature(reader, value, name, &owner);
            reader
                .context
                .classes
                .borrow_mut()
                .embedded_comparisons
                .insert((ty, name.clone()), Arc::new(declaration.bound(1)));
        }
    }
}
