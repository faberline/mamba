//! Original descriptor declarations retain generic bindings and decorator origin.
use super::field_types::Reader;
use crate::infrastructure::mypy::{
    classes::{Abstractness, Class, Field, Method, MethodKind},
    signatures::Signature,
    types::Type,
};
use serde_json::Value;
use std::sync::Arc;

pub(super) fn signature(
    reader: &mut Reader<'_>,
    value: &Value,
    name: &str,
    owner: &str,
) -> Signature {
    let mut signature = if value[".class"] == "Overloaded" {
        let mut root = Signature::empty(name.into(), Type::Any);
        root.overloads = value["items"]
            .as_array()
            .expect("descriptor overloads")
            .iter()
            .map(|variant| Arc::new(signature(reader, variant, name, owner)))
            .collect::<Vec<_>>()
            .into();
        root
    } else {
        reader.signature(value)
    };
    signature.name = Some(name.into());
    signature.native_symbol = None;
    signature.constructor = false;
    signature.owner = Some(owner.rsplit('.').next().expect("method owner").into());
    signature.definition_module = Some(owner.rsplit_once('.').expect("method module").0.into());
    signature
}

pub(super) fn populate(class: &mut Class, data: &Value, reader: &mut Reader<'_>) {
    let declaration = &data["class_property_descriptor"];
    let fullname = declaration["fullname"]
        .as_str()
        .expect("descriptor identity");
    let name = fullname.rsplit('.').next().expect("descriptor name");
    let id = reader.context.declare_class(fullname, name, 0);
    let mut descriptor = (*reader.context.class(id)).clone();
    descriptor.generic = declaration["generic"]
        .as_array()
        .expect("descriptor parameters")
        .iter()
        .map(|value| reader.ty(value))
        .collect();
    descriptor.scalar_bases = declaration["bases"]
        .as_array()
        .expect("descriptor bases")
        .iter()
        .map(|value| reader.ty(value))
        .collect();
    for (name, value) in declaration["methods"]
        .as_object()
        .expect("descriptor methods")
    {
        descriptor.methods.insert(
            name.clone(),
            Method {
                signature: Arc::new(signature(reader, &value["type"], name, fullname)),
                kind: MethodKind::Instance,
                setter: None,
                final_method: false,
                abstractness: Abstractness::Concrete,
            },
        );
    }
    reader.context.replace_class(id, descriptor);
    for (name, value) in data["class_properties"]
        .as_object()
        .expect("model class properties")
    {
        if value["node_kind"] == "Decorator" {
            class.decorated_fields.insert(name.clone());
        }
        class.fields.insert(
            name.clone(),
            Field {
                ty: Some(reader.ty(&value["type"])),
                needs_annotation: false,
                class_access: true,
                readonly: value["final"] == true,
                final_attribute: value["final"] == true,
                class_var: value["class_var"] == true,
                abstract_field: false,
            },
        );
    }
}
