//! Names and constructor flags retain the original serialized callable identity.
use super::*;
use crate::infrastructure::mypy::signatures::Signature;

pub(super) fn instance(value: &Value) -> Option<&Value> {
    if value[".class"] == "Overloaded" {
        return value["items"].as_array()?.first().and_then(instance);
    }
    value.get("instance_type").filter(|value| !value.is_null())
}

pub(super) fn read(
    reader: &mut Reader<'_>,
    value: &Value,
    module: &str,
    owner: Option<&str>,
) -> Signature {
    let mut signature = reader.signature(value);
    signature.native_symbol = None;
    signature.definition_module = Some(module.into());
    signature.owner = owner.map(String::from);
    signature.constructor = instance(value).is_some();
    if value[".class"] == "Overloaded" {
        signature.overloads = value["items"]
            .as_array()
            .expect("builtin overloads")
            .iter()
            .map(|value| Arc::new(read(reader, value, module, owner)))
            .collect::<Vec<_>>()
            .into();
        signature.name = signature.overloads.last().unwrap().name.clone();
    } else {
        signature.name = value["name"].as_str().map(|name| {
            name.split(" of ")
                .next()
                .unwrap()
                .rsplit('.')
                .next()
                .unwrap()
                .to_owned()
        });
    }
    signature
}
