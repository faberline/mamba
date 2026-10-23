//! Builtin model symbols retain their real constructor and typed dictionary fields.
use super::field_types::Reader;
use crate::infrastructure::mypy::{
    classes::{typed_dict, Abstractness, Field, Method, MethodKind},
    types::{Context, Type},
};
use mamba_language::application::quality_syntax::PythonTarget;
use serde_json::Value;
use std::{collections::BTreeMap, sync::Arc, sync::OnceLock};

fn data(target: PythonTarget) -> &'static Value {
    static DATA: OnceLock<Value> = OnceLock::new();
    let data = DATA.get_or_init(|| {
        serde_json::from_str(include_str!("../../../../assets/pydantic/model-types.json"))
            .expect("locked model symbols are valid JSON")
    });
    &data["targets"][match target {
        PythonTarget::Py312 => "3.12",
        PythonTarget::Py313 => "3.13",
        PythonTarget::Py314 => "3.14",
    }]
}

impl Context {
    pub(in crate::infrastructure::mypy) fn pydantic_model_symbol(
        &self,
        name: &str,
        target: PythonTarget,
    ) -> Option<Type> {
        let slot = match target {
            PythonTarget::Py312 => 0,
            PythonTarget::Py313 => 1,
            PythonTarget::Py314 => 2,
        };
        let member = match name {
            "BaseModel" => 0,
            "ConfigDict" => 1,
            _ => return None,
        };
        if let Some(types) = self.classes.borrow().pydantic_models[slot] {
            return Some(types[member]);
        }
        let data = data(target);
        let aliases = data["aliases"].as_object().expect("model type aliases");
        let mut reader = Reader {
            context: self,
            original_aliases: aliases
                .iter()
                .map(|(name, value)| (name.clone(), value.clone()))
                .collect(),
            aliases_ready: false,
            aliases: aliases
                .keys()
                .enumerate()
                .map(|(index, name)| {
                    let id = self.declare_alias(
                        &format!("<pydantic-model:{slot}>:{name}"),
                        name.rsplit('.').next().expect("alias name"),
                        u32::try_from(index).expect("alias index"),
                        false,
                        false,
                    );
                    (name.clone(), id)
                })
                .collect(),
            variables: BTreeMap::new(),
        };
        for (name, value) in aliases {
            let ty = reader.ty(value);
            self.set_embedded_alias_target(reader.aliases[name], ty);
        }
        reader.aliases_ready = true;
        let model = &data["model_types"]["base_model"];
        let id = self.declare_class(
            model["fullname"].as_str().expect("base model identity"),
            "BaseModel",
            0,
        );
        let mut class = (*self.class(id)).clone();
        class.scalar_bases = vec![Type::Object];
        class.plugin = true;
        let mut signature = reader.signature(&model["init_type"]);
        signature.name = Some("__init__".into());
        signature.native_symbol = None;
        signature.definition_module = Some("pydantic.main".into());
        signature.owner = Some("BaseModel".into());
        class.methods.insert(
            "__init__".into(),
            Method {
                signature: Arc::new(signature),
                kind: MethodKind::Instance,
                setter: None,
                final_method: false,
                abstractness: Abstractness::Concrete,
            },
        );
        let base = class.object;
        self.pydantic_model_methods(&mut class, target);
        self.replace_class(id, class);
        let config = &data["model_types"]["config_dict"]["type"];
        let id = self.declare_class(&format!("<pydantic-config:{slot}>"), "ConfigDict", 0);
        let mut class = (*self.class(id)).clone();
        class.typed_dict = Some(typed_dict::Definition {
            closed: false,
            items: config["items"]
                .as_array()
                .expect("configuration fields")
                .iter()
                .map(|item| {
                    let name = item[0].as_str().expect("configuration field name");
                    let contains = |key: &str| {
                        config[key]
                            .as_array()
                            .expect("configuration field flags")
                            .iter()
                            .any(|value| value.as_str() == Some(name))
                    };
                    typed_dict::Item {
                        name: name.into(),
                        ty: reader.ty(&item[1]),
                        required: contains("required_keys"),
                        readonly: contains("readonly_keys"),
                    }
                })
                .collect(),
        });
        let configuration = class.object;
        self.replace_class(id, class);
        static CONFIG_FIELD: OnceLock<Value> = OnceLock::new();
        let field = &CONFIG_FIELD.get_or_init(|| {
            serde_json::from_str(include_str!(
                "../../../../assets/pydantic/model-config-field.json"
            ))
            .expect("locked model config field")
        })["targets"][match target {
            PythonTarget::Py312 => "3.12",
            PythonTarget::Py313 => "3.13",
            PythonTarget::Py314 => "3.14",
        }];
        assert_eq!(field["type"][".class"], "TypeAliasType");
        assert_eq!(field["type"]["type_ref"], "pydantic.config.ConfigDict");
        let base_id = self.class_id(base).expect("BaseModel identity");
        let mut base_class = (*self.class(base_id)).clone();
        base_class.fields.insert(
            "model_config".into(),
            Field {
                ty: Some(
                    self.instance_type(configuration)
                        .expect("ConfigDict instance"),
                ),
                needs_annotation: false,
                class_access: true,
                readonly: field["final"] == true,
                final_attribute: field["final"] == true,
                class_var: field["class_var"] == true,
                abstract_field: false,
            },
        );
        self.replace_class(base_id, base_class);
        self.classes.borrow_mut().pydantic_models[slot] = Some([base, configuration]);
        Some([base, configuration][member])
    }
}
