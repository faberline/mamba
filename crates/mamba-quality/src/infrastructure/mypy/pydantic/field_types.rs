//! Locked library symbols use the same structural types as project declarations.
pub(super) use crate::infrastructure::mypy::declarations::Reader;
use crate::infrastructure::mypy::{
    signatures::Signature,
    types::{Context, Type},
};
use mamba_language::application::quality_syntax::PythonTarget;
use serde_json::Value;
use std::{collections::BTreeMap, sync::Arc, sync::OnceLock};

fn data(target: PythonTarget) -> &'static Value {
    static DATA: OnceLock<Value> = OnceLock::new();
    let data = DATA.get_or_init(|| {
        serde_json::from_str(include_str!("../../../../assets/pydantic/field-types.json"))
            .expect("locked Pydantic symbols are valid JSON")
    });
    &data["targets"][match target {
        PythonTarget::Py312 => "3.12",
        PythonTarget::Py313 => "3.13",
        PythonTarget::Py314 => "3.14",
    }]
}

impl Context {
    pub(in crate::infrastructure::mypy) fn pydantic_field_function(
        &self,
        target: PythonTarget,
    ) -> Arc<Signature> {
        let slot = match target {
            PythonTarget::Py312 => 0,
            PythonTarget::Py313 => 1,
            PythonTarget::Py314 => 2,
        };
        if let Some(signature) = self.classes.borrow().pydantic_fields[slot].clone() {
            return signature;
        }
        let data = data(target);
        let aliases = data["aliases"].as_object().expect("locked aliases");
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
                    let short = name.rsplit('.').next().expect("alias name");
                    let id = self.declare_alias(
                        &format!("<pydantic:{slot}>:{name}"),
                        short,
                        u32::try_from(index).expect("alias index"),
                        false,
                        false,
                    );
                    (name.clone(), id)
                })
                .collect(),
            variables: BTreeMap::new(),
        };
        for (name, target) in aliases {
            let target = reader.ty(target);
            self.set_embedded_alias_target(reader.aliases[name], target);
        }
        reader.aliases_ready = true;
        let mut signature = Signature::empty("Field".into(), Type::Any);
        signature.constructor = false;
        signature.definition_module = Some("pydantic.fields".into());
        signature.native_symbol = Some("pydantic.fields.Field");
        signature.overloads = data["serialized_type"]["items"]
            .as_array()
            .expect("locked overloads")
            .iter()
            .map(|variant| Arc::new(reader.signature(variant)))
            .collect::<Vec<_>>()
            .into();
        let signature = Arc::new(signature);
        self.classes.borrow_mut().pydantic_fields[slot] = Some(signature.clone());
        signature
    }
}

pub(in crate::infrastructure::mypy) fn is_field(signature: &Signature) -> bool {
    signature.native_symbol == Some("pydantic.fields.Field")
}

pub(in crate::infrastructure::mypy) fn overload_hint(
    signature: &Signature,
    target: PythonTarget,
) -> Option<String> {
    is_field(signature).then(|| {
        let variants = data(target)["variants"]
            .as_array()
            .expect("locked rendered signatures")
            .iter()
            .map(|value| format!("    {}", value.as_str().expect("rendered signature")))
            .collect::<Vec<_>>()
            .join("\n");
        format!("Possible overload variants:\n{variants}")
    })
}
