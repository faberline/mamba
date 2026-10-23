//! Scalar comparison and addition members retain their pinned declarations.
use super::*;
use crate::infrastructure::mypy::{declarations::Reader, signatures::Signature};
use mamba_language::application::quality_syntax::PythonTarget;
use std::sync::Arc;

impl Context {
    pub(super) fn aggregation_scalar_method(
        &self,
        ty: Type,
        name: &str,
        target: PythonTarget,
    ) -> Option<Arc<Signature>> {
        let scalar = match self.node(ty).as_deref() {
            Some(Node::Literal(value) | Node::LiteralValue(value)) => value.base(),
            _ => ty,
        };
        let scalar = if scalar == Type::Bool {
            Type::Int
        } else {
            scalar
        };
        let owner = match scalar {
            Type::Int => "int",
            Type::Float => "float",
            Type::Complex => "complex",
            Type::Str => "str",
            Type::Bytes => "bytes",
            _ => return None,
        };
        let data = super::aggregation::data(target);
        let value = data["scalar_methods"][owner].get(name)?;
        let cache_key = (ty, name.into());
        if let Some(signature) = self.classes.borrow().collection_methods.get(&cache_key) {
            return Some(signature.clone());
        }
        let mut reader = Reader {
            context: self,
            aliases: BTreeMap::new(),
            original_aliases: BTreeMap::new(),
            variables: BTreeMap::new(),
            aliases_ready: true,
        };
        let mut signature = reader.signature(value).bound(1);
        normalize(&mut signature, name, owner);
        let signature = Arc::new(signature);
        self.classes
            .borrow_mut()
            .collection_methods
            .insert(cache_key, signature.clone());
        Some(signature)
    }
}

fn normalize(signature: &mut Signature, name: &str, owner: &str) {
    signature.name = Some(name.into());
    signature.native_symbol = None;
    signature.owner = Some(owner.into());
    signature.definition_module = Some("builtins".into());
    signature.bound_receiver = Some("self".into());
    for variant in Arc::make_mut(&mut signature.overloads) {
        normalize(Arc::make_mut(variant), name, owner);
    }
}
