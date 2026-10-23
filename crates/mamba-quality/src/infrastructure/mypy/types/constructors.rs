//! Constructor selection follows the class MRO and preserves allocation returns.
use super::super::signatures::{Kind, Signature};
use super::*;
use mamba_language::application::quality_syntax::PythonTarget;
use std::sync::Arc;

impl Context {
    pub(in crate::infrastructure::mypy) fn constructor_member(
        &self,
        receiver: Type,
    ) -> Option<(&'static str, u32)> {
        for id in self.class_lineage(self.class_id(receiver)?) {
            let class = self.class(id);
            for name in ["__init__", "__new__"] {
                if class.methods.contains_key(name) {
                    return Some((name, id));
                }
            }
        }
        None
    }

    pub(super) fn constructor_signature(
        &self,
        receiver: Type,
        instance: Type,
        target: PythonTarget,
    ) -> Arc<Signature> {
        let class = self.class(self.class_id(receiver).unwrap());
        let Some((name, owner)) = self.constructor_member(receiver) else {
            return Arc::new(Signature::empty(class.name.clone(), instance));
        };
        let declaration = self.class(owner);
        if let Some(scalar) = self.builtin_scalar(declaration.instance) {
            let signature = self
                .builtin_constructor(scalar, target)
                .expect("builtin constructor has a locked signature");
            let returned = self.builtin_scalar(receiver).unwrap_or(instance);
            return Arc::new(signature.constructor_for(&class.name, returned));
        }
        let method = self.specialize_method(&declaration.methods[name], instance, owner);
        if name == "__new__" {
            self.allocation_constructor(&method.signature.bound(1), &class.name, instance, owner)
        } else {
            self.initializer_constructor(&method.signature, &class.name, instance)
        }
    }

    fn initializer_constructor(
        &self,
        signature: &Signature,
        name: &str,
        instance: Type,
    ) -> Arc<Signature> {
        // The collector's canonical instance is an implicit receiver placeholder.
        // Preserve the default generic instance instead of treating it as specialization.
        let returns = signature
            .parameters
            .first()
            .filter(|parameter| {
                matches!(
                    parameter.kind,
                    Kind::PositionalOnly | Kind::PositionalOrKeyword
                )
            })
            .and_then(|parameter| parameter.ty)
            .filter(|&receiver| {
                matches!(
                    self.node(receiver).as_deref(),
                    Some(Node::Instance(_) | Node::AppliedClass { value: false, .. })
                ) && self.class_id(instance).is_some_and(|id| {
                    receiver != self.class(id).instance
                        && !self.class(id).protocol
                        && self
                            .class_id(receiver)
                            .is_some_and(|receiver| self.nominal_accepts(id, receiver))
                })
            })
            .unwrap_or(instance);
        let mut result = signature.bound(1).constructor_for(name, returns);
        result.overloads = signature
            .overloads
            .iter()
            .map(|variant| self.initializer_constructor(variant, name, instance))
            .collect();
        Arc::new(result)
    }

    fn allocation_constructor(
        &self,
        signature: &Signature,
        name: &str,
        instance: Type,
        owner: u32,
    ) -> Arc<Signature> {
        let declared = if signature.return_explicit {
            signature.returns.unwrap_or(instance)
        } else {
            instance
        };
        let returns =
            if self.class_id(declared) == Some(owner) && self.class_id(instance) != Some(owner) {
                instance
            } else {
                declared
            };
        let mut result = signature.constructor_for(name, returns);
        result.overloads = signature
            .overloads
            .iter()
            .map(|variant| self.allocation_constructor(variant, name, instance, owner))
            .collect();
        Arc::new(result)
    }
}
