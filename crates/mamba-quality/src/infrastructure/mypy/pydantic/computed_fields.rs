//! Decorators retain original generic overloads without running model code.
use super::{class_properties::signature, field_types::Reader};
use crate::infrastructure::mypy::{
    classes::{Abstractness, Method, MethodKind},
    signatures::Signature,
    types::{Context, Type},
};
use mamba_language::application::quality_syntax::PythonTarget;
use serde_json::Value;
use std::sync::Arc;

fn slot(target: PythonTarget) -> usize {
    match target {
        PythonTarget::Py312 => 0,
        PythonTarget::Py313 => 1,
        PythonTarget::Py314 => 2,
    }
}

fn function(mut signature: Signature) -> Signature {
    signature.name = Some("computed_field".into());
    signature.owner = None;
    signature.native_symbol = Some("pydantic.fields.computed_field");
    signature.overloads = signature
        .overloads
        .iter()
        .map(|variant| Arc::new(function((**variant).clone())))
        .collect();
    signature
}

pub(super) fn populate(data: &Value, reader: &mut Reader<'_>, target: PythonTarget) {
    let declaration = function(signature(
        reader,
        &data["computed_field"],
        "computed_field",
        "pydantic.fields",
    ));
    reader.context.classes.borrow_mut().pydantic_computed_fields[slot(target)] =
        Some(Arc::new(declaration));
    let data = &data["cached_property"];
    let fullname = data["fullname"].as_str().expect("cached property identity");
    let id = reader.context.declare_class(fullname, "cached_property", 0);
    let mut class = (*reader.context.class(id)).clone();
    class.generic = data["generic"]
        .as_array()
        .expect("cached property parameters")
        .iter()
        .map(|value| reader.ty(value))
        .collect();
    class.scalar_bases = vec![Type::Object];
    for (name, value) in data["methods"]
        .as_object()
        .expect("cached property methods")
    {
        class.methods.insert(
            name.clone(),
            Method {
                signature: Arc::new(signature(reader, value, name, fullname)),
                kind: MethodKind::Instance,
                setter: None,
                final_method: false,
                abstractness: Abstractness::Concrete,
            },
        );
    }
    reader.context.replace_class(id, class);
}

impl Context {
    pub(in crate::infrastructure::mypy) fn pydantic_computed_field(
        &self,
        target: PythonTarget,
    ) -> Arc<Signature> {
        self.pydantic_model_symbol("BaseModel", target)
            .expect("base model exists");
        self.classes.borrow().pydantic_computed_fields[slot(target)]
            .clone()
            .expect("computed field declaration exists")
    }

    pub(in crate::infrastructure::mypy) fn cached_property_class(
        &self,
        target: PythonTarget,
    ) -> Type {
        self.pydantic_model_symbol("BaseModel", target)
            .expect("base model exists");
        let id = self.declare_class("functools.cached_property", "cached_property", 0);
        self.class(id).object
    }
}
