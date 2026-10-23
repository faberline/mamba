//! Root classes retain their own initializer and inherited project plugin data.
use super::{class_properties::signature, field_types::Reader, Field, Model};
use crate::infrastructure::mypy::{
    classes::{Abstractness, Field as ClassField, Method, MethodKind},
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

pub(super) fn populate(data: &Value, reader: &mut Reader<'_>, target: PythonTarget) {
    let fullname = data["fullname"].as_str().expect("root model identity");
    let id = reader.context.declare_class(fullname, "RootModel", 0);
    let mut class = (*reader.context.class(id)).clone();
    class.generic = data["generic"]
        .as_array()
        .expect("root model variables")
        .iter()
        .map(|value| reader.ty(value))
        .collect();
    class.base_types = data["bases"]
        .as_array()
        .expect("root model bases")
        .iter()
        .map(|value| reader.ty(value))
        .collect();
    class.bases = class
        .base_types
        .iter()
        .filter_map(|&ty| reader.context.class_id(ty))
        .collect();
    class.plugin = true;
    let root_type = reader.ty(&data["root_type"]);
    class.fields.insert(
        "root".into(),
        ClassField {
            ty: Some(root_type),
            needs_annotation: false,
            class_access: false,
            readonly: false,
            final_attribute: false,
            class_var: false,
            abstract_field: false,
        },
    );
    for (name, value) in data["methods"].as_object().expect("root model methods") {
        class.methods.insert(
            name.clone(),
            Method {
                signature: Arc::new(signature(reader, &value["type"], name, fullname)),
                kind: if value["class_method"] == true {
                    MethodKind::Class
                } else {
                    MethodKind::Instance
                },
                setter: None,
                final_method: false,
                abstractness: Abstractness::Concrete,
            },
        );
    }
    let root = &data["metadata"]["fields"]["root"];
    reader
        .context
        .classes
        .borrow_mut()
        .embedded_pydantic_models
        .insert(
            id,
            Model {
                name: "RootModel".into(),
                fields: vec![Field {
                    name: "root".into(),
                    alias: None,
                    ty: Some(root_type),
                    required: root["has_default"] == false,
                    frozen: root["is_frozen"] == true,
                    dynamic_alias: root["has_dynamic_alias"] == true,
                }],
                frozen: false,
                populate_by_name: false,
                validate_by_name: false,
                validate_by_alias: None,
                alias_generator: false,
                from_attributes: None,
                is_root_model: true,
                configuration: Default::default(),
            },
        );
    reader.context.classes.borrow_mut().pydantic_root_models[slot(target)] = Some(class.object);
    reader.context.replace_class(id, class);
}

impl Context {
    pub(in crate::infrastructure::mypy) fn pydantic_root_model(
        &self,
        target: PythonTarget,
    ) -> Type {
        self.pydantic_model_symbol("BaseModel", target)
            .expect("base model exists");
        self.classes.borrow().pydantic_root_models[slot(target)].expect("root model exists")
    }
}
