//! Constructor values retain the pinned unbound members of their classes.
use super::*;
use crate::infrastructure::mypy::{declarations::Reader, signatures::Signature};
use mamba_language::application::quality_syntax::PythonTarget;
use serde_json::Value;
use std::sync::{Arc, OnceLock};
mod class_methods;

impl Context {
    pub(in crate::infrastructure::mypy) fn argument_has_unresolved_variable(
        &self,
        ty: Type,
        unresolved: &[Type],
    ) -> bool {
        if unresolved.is_empty() {
            return false;
        }
        let mut variables = Vec::new();
        self.collect_variables(ty, &mut variables);
        variables
            .iter()
            .any(|variable| unresolved.contains(variable))
    }

    pub(in crate::infrastructure::mypy) fn builtin_constructor_member(
        &self,
        ty: Type,
        name: &str,
        target: PythonTarget,
    ) -> Option<Arc<Signature>> {
        let node = self.node(ty)?;
        let Node::Callable(constructor) = node.as_ref() else {
            return None;
        };
        if !constructor.constructor || constructor.definition_module.as_deref() != Some("builtins")
        {
            return None;
        }
        let class = constructor.name.as_deref()?;
        static DATA: OnceLock<Value> = OnceLock::new();
        let key = match target {
            PythonTarget::Py312 => "3.12",
            PythonTarget::Py313 => "3.13",
            PythonTarget::Py314 => "3.14",
        };
        let data = DATA.get_or_init(|| {
            serde_json::from_str(include_str!(
                "../../../../assets/stdlib/builtin-class-members.json"
            ))
            .expect("locked unbound builtin members")
        });
        let Some(value) = data["targets"][key]["classes"][class]["methods"].get(name) else {
            return class_methods::lookup(self, class, name, target);
        };
        let cache_key = (ty, name.to_owned());
        if let Some(method) = self.classes.borrow().collection_methods.get(&cache_key) {
            return Some(method.clone());
        }
        let mut reader = Reader {
            context: self,
            aliases: BTreeMap::new(),
            original_aliases: BTreeMap::new(),
            variables: BTreeMap::new(),
            aliases_ready: true,
        };
        let mut method = reader.signature(value);
        normalize(&mut method, name, class);
        let method = Arc::new(method);
        self.classes
            .borrow_mut()
            .collection_methods
            .insert(cache_key, method.clone());
        Some(method)
    }
}

fn normalize(method: &mut Signature, name: &str, owner: &str) {
    method.name = Some(name.into());
    method.owner = Some(owner.into());
    method.constructor = false;
    method.definition_module = Some("builtins".into());
    method.native_symbol = Some("builtins.unbound_member");
    for variant in Arc::make_mut(&mut method.overloads) {
        normalize(Arc::make_mut(variant), name, owner);
    }
}
