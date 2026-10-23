//! Container ordering and addition specialize pinned members with element types.
use super::*;
use crate::infrastructure::mypy::{declarations::Reader, signatures::Signature};
use mamba_language::application::quality_syntax::PythonTarget;
use std::sync::Arc;

impl Context {
    pub(super) fn aggregation_container_method(
        &self,
        ty: Type,
        name: &str,
        target: PythonTarget,
    ) -> Option<Arc<Signature>> {
        let (owner, item) = match self.node(ty).as_deref()? {
            Node::Container(Container::List, arguments) => ("list", arguments[0]),
            Node::Tuple(items) => ("tuple", self.union(items.clone())),
            Node::VariadicTuple(item) => ("tuple", *item),
            _ => return None,
        };
        let data = super::aggregation::data(target);
        let class = &data["container_methods"][owner];
        let value = class["methods"].get(name)?;
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
        let variable = reader.ty(&class["generic"][0]);
        let signature = reader.signature(value);
        let mut signature = self
            .substitute_callable(&signature, &BTreeMap::from([(variable, item)]))
            .bound(1);
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
