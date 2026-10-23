//! Class-object calls use declared instance constructors without fresh binders.
use super::super::signatures::{Kind, Parameter, Signature};
use super::*;
use mamba_language::application::quality_syntax::PythonTarget;
use std::sync::Arc;

impl Context {
    pub(in crate::infrastructure::mypy) fn type_object_constructors_for(
        &self,
        ty: Type,
        target: PythonTarget,
    ) -> Option<Vec<Arc<Signature>>> {
        match self.node(ty).as_deref() {
            Some(Node::Container(Container::TypeObject, items)) => self.type_item_constructors(
                items[0],
                target,
                &mut std::collections::BTreeSet::new(),
            ),
            Some(Node::Union(items)) => {
                let mut signatures = Vec::new();
                for &item in items {
                    signatures.extend(self.type_object_constructors_for(item, target)?);
                }
                Some(signatures)
            }
            _ => None,
        }
    }

    fn type_item_constructors(
        &self,
        item: Type,
        target: PythonTarget,
        active: &mut std::collections::BTreeSet<Type>,
    ) -> Option<Vec<Arc<Signature>>> {
        if !active.insert(item) {
            return None;
        }
        let result = match self.node(item).as_deref() {
            Some(Node::Union(items)) => {
                let mut signatures = Vec::new();
                for &item in items {
                    signatures.extend(self.type_item_constructors(item, target, active)?);
                }
                Some(signatures)
            }
            Some(Node::Variable(variable)) => {
                let signatures = self.type_item_constructors(variable.bound, target, active)?;
                Some(
                    signatures
                        .iter()
                        .map(|signature| Arc::new(self.type_object_result(signature, item, None)))
                        .collect(),
                )
            }
            _ if item.is_any() => {
                let mut signature = Signature::empty("Any".into(), Type::Any);
                signature.name = None;
                signature.parameters = [Kind::VariadicPositional, Kind::VariadicKeyword]
                    .into_iter()
                    .map(|kind| Parameter {
                        name: String::new(),
                        kind,
                        required: false,
                        ty: Some(Type::Any),
                    })
                    .collect();
                Some(vec![Arc::new(signature)])
            }
            _ if self.node(item).is_none() => self
                .builtin_constructor(item, target)
                .map(|signature| vec![signature]),
            _ => self
                .class_constructor_for(self.class_value_type(item), target)
                .map(|signature| {
                    vec![Arc::new(self.type_object_result(
                        &signature,
                        signature.returns.unwrap_or(item),
                        Some(item),
                    ))]
                }),
        };
        active.remove(&item);
        result
    }

    fn type_object_result(
        &self,
        signature: &Signature,
        returns: Type,
        receiver: Option<Type>,
    ) -> Signature {
        let origin = receiver
            .and_then(|receiver| self.constructor_member(receiver))
            .and_then(|(_, id)| {
                let class = self.class(id);
                let module = class
                    .namespace
                    .split_once(':')
                    .map_or(class.namespace.as_str(), |(_, module)| module);
                (!matches!(module, "<builtins>" | "builtins" | "typing"))
                    .then(|| module.to_string())
            });
        Signature {
            returns: Some(returns),
            generic: Arc::from([]),
            definition_module: origin.or_else(|| signature.definition_module.clone()),
            overloads: signature
                .overloads
                .iter()
                .map(|variant| {
                    Arc::new(self.type_object_result(
                        variant,
                        if receiver.is_some() {
                            variant.returns.unwrap_or(returns)
                        } else {
                            returns
                        },
                        receiver,
                    ))
                })
                .collect(),
            ..signature.clone()
        }
    }
}
