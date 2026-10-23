//! Collection constructors retain the pinned overloads and target identity.
use super::*;
use crate::infrastructure::mypy::{declarations::Reader, signatures::Signature};
use mamba_language::application::quality_syntax::PythonTarget;
use serde_json::Value;
use std::sync::{Arc, OnceLock};

impl Context {
    pub(in crate::infrastructure::mypy) fn collection_constructor(
        &self,
        name: &str,
        target: PythonTarget,
    ) -> Option<Arc<Signature>> {
        static DATA: OnceLock<Value> = OnceLock::new();
        let target_key = match target {
            PythonTarget::Py312 => "3.12",
            PythonTarget::Py313 => "3.13",
            PythonTarget::Py314 => "3.14",
        };
        let data = DATA.get_or_init(|| {
            serde_json::from_str(include_str!(
                "../../../../assets/stdlib/collection-constructors.json"
            ))
            .expect("locked collection constructors")
        });
        let data = &data["targets"][target_key];
        let value = data["constructors"].get(name)?;
        let key = format!("{target:?}:constructor:{name}");
        if let Some(signature) = self.classes.borrow().collection_functions.get(&key) {
            return Some(signature.clone());
        }
        self.standard_collection("typing", "Iterable", target)?;
        let mut reader = Reader {
            context: self,
            aliases: BTreeMap::new(),
            original_aliases: BTreeMap::new(),
            variables: BTreeMap::new(),
            aliases_ready: true,
        };
        self.load_constructor_declarations(data, target, &mut reader);
        let mut signature = reader.signature(value);
        signature.name = Some(name.into());
        signature.constructor = true;
        signature.native_symbol = None;
        signature.definition_module = Some("builtins".into());
        for variant in Arc::make_mut(&mut signature.overloads) {
            let variant = Arc::make_mut(variant);
            variant.name = Some(name.into());
            variant.constructor = true;
            variant.native_symbol = None;
            variant.definition_module = Some("builtins".into());
        }
        let signature = Arc::new(signature);
        self.classes
            .borrow_mut()
            .collection_functions
            .insert(key, signature.clone());
        Some(signature)
    }
}
