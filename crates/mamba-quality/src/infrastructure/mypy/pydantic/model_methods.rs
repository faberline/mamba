//! Locked model methods use ordinary inheritance, call binding, and property checks.
use super::field_types::Reader;
use crate::infrastructure::mypy::{
    classes::{Abstractness, Class, Method, MethodKind},
    types::{Context, Type},
};
use mamba_language::application::quality_syntax::PythonTarget;
use serde_json::Value;
use std::{collections::BTreeMap, sync::Arc, sync::OnceLock};

fn data(target: PythonTarget) -> &'static Value {
    static DATA: OnceLock<Value> = OnceLock::new();
    let data = DATA.get_or_init(|| {
        serde_json::from_str(include_str!(
            "../../../../assets/pydantic/model-methods.json"
        ))
        .expect("locked model methods are valid JSON")
    });
    &data["targets"][match target {
        PythonTarget::Py312 => "3.12",
        PythonTarget::Py313 => "3.13",
        PythonTarget::Py314 => "3.14",
    }]
}

impl Context {
    pub(in crate::infrastructure::mypy) fn pydantic_info_class(
        &self,
        name: &str,
        target: PythonTarget,
    ) -> Option<Type> {
        let value = data(target)["model_types"]["info_classes"].get(name)?;
        self.pydantic_model_symbol("BaseModel", target)
            .expect("base model exists");
        let id = self.declare_class(
            value["fullname"].as_str().expect("information identity"),
            name,
            0,
        );
        Some(self.class(id).object)
    }

    pub(in crate::infrastructure::mypy) fn pydantic_root_getattr(
        &self,
        target: PythonTarget,
    ) -> Type {
        let returned = data(target)["model_types"]["root_getattr"]["ret_type"]
            .as_str()
            .expect("root attribute type");
        Self::builtin_name(
            returned
                .strip_prefix("builtins.")
                .expect("builtin root return"),
        )
        .expect("root return exists")
    }

    pub(super) fn pydantic_model_methods(&self, class: &mut Class, target: PythonTarget) {
        let data = data(target);
        let aliases = data["aliases"].as_object().expect("method type aliases");
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
                .enumerate()
                .map(|(index, name)| {
                    let id = self.declare_alias(
                        &format!("<pydantic-methods:{target:?}>:{name}"),
                        name.rsplit('.').next().expect("alias name"),
                        u32::try_from(index).expect("alias index"),
                        false,
                        false,
                    );
                    (name.clone(), id)
                })
                .collect(),
        };
        for (name, target) in aliases {
            let ty = reader.alias_target(name, target);
            self.set_embedded_alias_target(reader.aliases[name], ty);
        }
        reader.aliases_ready = true;
        super::class_properties::populate(class, &data["model_types"], &mut reader);
        super::info_members::populate(&data["model_types"], &mut reader);
        super::computed_fields::populate(&data["model_types"], &mut reader, target);
        super::root_models::populate(&data["model_types"]["root_model"], &mut reader, target);
        let schema = &data["model_types"]["schema_generator"];
        let id = self.declare_class(
            schema["fullname"]
                .as_str()
                .expect("schema generator identity"),
            "GenerateJsonSchema",
            0,
        );
        let mut generator = (*self.class(id)).clone();
        generator.scalar_bases = vec![Type::Object];
        let mut initializer = reader.signature(&schema["init_type"]);
        initializer.name = Some("__init__".into());
        initializer.native_symbol = None;
        initializer.owner = Some(generator.name.clone());
        initializer.definition_module = Some("pydantic.json_schema".into());
        generator.methods.insert(
            "__init__".into(),
            Method {
                signature: Arc::new(initializer),
                kind: MethodKind::Instance,
                setter: None,
                final_method: false,
                abstractness: Abstractness::Concrete,
            },
        );
        self.replace_class(id, generator);
        for (name, value) in data["model_types"]["methods"]
            .as_object()
            .expect("model methods")
        {
            let mut signature = if value["node_kind"] == "Decorator" {
                reader.decorated_signature(&value["type"])
            } else {
                reader.signature(&value["type"])
            };
            signature.name = Some(name.clone());
            signature.native_symbol = None;
            signature.owner = Some(class.name.clone());
            signature.definition_module = Some("pydantic.main".into());
            let kind = if value["property"] == true {
                MethodKind::Property
            } else if value["class_method"] == true {
                MethodKind::Class
            } else if value["static_method"] == true {
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
                    abstractness: Abstractness::Concrete,
                },
            );
        }
    }

    pub(in crate::infrastructure::mypy) fn pydantic_schema_generator(
        &self,
        target: PythonTarget,
    ) -> Type {
        self.pydantic_model_symbol("BaseModel", target)
            .expect("base model exists");
        let schema = &data(target)["model_types"]["schema_generator"];
        let id = self.declare_class(
            schema["fullname"]
                .as_str()
                .expect("schema generator identity"),
            "GenerateJsonSchema",
            0,
        );
        self.class(id).object
    }
}
