//! Object comparison declarations retain the locked positional and return types.
use super::*;
use crate::infrastructure::mypy::{declarations::Reader, signatures::Signature};
use serde_json::Value;
use std::sync::{Arc, OnceLock};

impl Context {
    pub(super) fn object_comparison_method(&self, name: &str) -> Option<Arc<Signature>> {
        static DATA: OnceLock<Value> = OnceLock::new();
        let data = DATA.get_or_init(|| {
            serde_json::from_str(include_str!(
                "../../../../assets/stdlib/object-comparisons.json"
            ))
            .expect("locked object comparisons")
        });
        let value = data["targets"]["3.12"].get(name)?;
        let key = format!("object:comparison:{name}");
        if let Some(signature) = self.classes.borrow().collection_functions.get(&key) {
            return Some(signature.clone());
        }
        let mut reader = Reader {
            context: self,
            aliases: BTreeMap::new(),
            original_aliases: BTreeMap::new(),
            variables: BTreeMap::new(),
            aliases_ready: true,
        };
        let mut signature = reader.signature(value);
        signature.name = Some(name.into());
        signature.owner = Some("object".into());
        signature.native_symbol = None;
        signature.definition_module = Some("builtins".into());
        if let Some(first) = Arc::make_mut(&mut signature.parameters).first_mut() {
            first.name = "self".into();
        }
        let signature = Arc::new(signature);
        self.classes
            .borrow_mut()
            .collection_functions
            .insert(key, signature.clone());
        Some(signature)
    }
}
