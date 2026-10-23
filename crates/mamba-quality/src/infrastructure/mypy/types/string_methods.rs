//! String case methods use the pinned original signatures.
use super::*;
use crate::infrastructure::mypy::{declarations::Reader, signatures::Signature};
use mamba_language::application::quality_syntax::PythonTarget;
use serde_json::Value;
use std::sync::{Arc, OnceLock};

impl Context {
    pub(super) fn string_case_method(
        &self,
        ty: Type,
        name: &str,
        target: PythonTarget,
    ) -> Option<Arc<Signature>> {
        let scalar = match self.node(ty).as_deref() {
            Some(Node::Literal(value) | Node::LiteralValue(value)) => value.base(),
            _ => ty,
        };
        let scalar = self.builtin_scalar(scalar).unwrap_or(scalar);
        let bound = !self.is_class_value(ty);
        let owner = match scalar {
            Type::Str => "str",
            Type::Bytes => "bytes",
            _ => return None,
        };
        let version = match target {
            PythonTarget::Py312 => "3.12",
            PythonTarget::Py313 => "3.13",
            PythonTarget::Py314 => "3.14",
        };
        static DATA: OnceLock<Value> = OnceLock::new();
        let data = DATA.get_or_init(|| {
            serde_json::from_str(include_str!(
                "../../../../assets/stdlib/string-case-methods.json"
            ))
            .expect("locked string case methods")
        });
        let value = data["targets"][version][owner].get(name).or_else(|| {
            if owner != "str" || name != "join" {
                return None;
            }
            static JOIN_DATA: OnceLock<Value> = OnceLock::new();
            let joins = JOIN_DATA.get_or_init(|| {
                serde_json::from_str(include_str!(
                    "../../../../assets/stdlib/string-join-methods.json"
                ))
                .expect("locked string join declarations")
            });
            joins["targets"][version]["classes"][owner]["methods"].get(name)
        })?;
        let cache_key = format!("{target:?}:string-case:{owner}:{name}:{bound}");
        if let Some(signature) = self.classes.borrow().collection_functions.get(&cache_key) {
            return Some(signature.clone());
        }
        let mut reader = Reader {
            context: self,
            aliases: BTreeMap::new(),
            original_aliases: BTreeMap::new(),
            variables: BTreeMap::new(),
            aliases_ready: true,
        };
        let mut signature = reader.signature(value).bound(usize::from(bound));
        signature.name = Some(name.into());
        signature.owner = Some(owner.into());
        signature.definition_module = None;
        signature.bound_receiver = bound.then(|| "self".into());
        let signature = Arc::new(signature);
        self.classes
            .borrow_mut()
            .collection_functions
            .insert(cache_key, signature.clone());
        Some(signature)
    }
}
