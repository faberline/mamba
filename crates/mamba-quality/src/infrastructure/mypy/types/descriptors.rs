//! Descriptor calls resolve through declared method types without running Python.
use super::super::{classes::MethodKind, signatures::Signature};
use super::*;
use std::sync::Arc;

impl Context {
    pub(in crate::infrastructure::mypy) fn descriptor_implicit(
        &self,
        receiver: Type,
        name: &str,
    ) -> bool {
        self.class_member_owner(receiver, name)
            .is_some_and(|owner| {
                self.classes
                    .borrow()
                    .embedded_implicit_fields
                    .get(&owner)
                    .is_some_and(|fields| fields.contains(name))
                    || self
                        .class(owner)
                        .dataclass
                        .as_ref()
                        .is_some_and(|definition| {
                            !definition.transformed
                                && definition
                                    .attributes
                                    .iter()
                                    .any(|attribute| attribute.name == name && attribute.required)
                        })
            })
    }

    pub(in crate::infrastructure::mypy) fn descriptor_member(
        &self,
        receiver: Type,
        name: &str,
    ) -> Option<Type> {
        if !self.is_class_value(receiver) && self.descriptor_implicit(receiver, name) {
            return None;
        }
        let owner = self.class_member_owner(receiver, name)?;
        if self
            .classes
            .borrow()
            .embedded_class_slots
            .get(&owner)
            .is_some_and(|slots| slots.contains(name))
        {
            return None;
        }
        let member = self.class(owner).fields.get(name)?.ty?;
        let descriptor = self.substitute(member, &self.class_solutions(receiver, owner));
        if matches!(self.node(descriptor).as_deref(), Some(Node::Union(_))) {
            return Some(descriptor);
        }
        self.class_id(descriptor)?;
        (!self.is_class_value(descriptor)).then_some(descriptor)
    }

    pub(in crate::infrastructure::mypy) fn descriptor_get_signature(
        &self,
        descriptor: Type,
        receiver: Type,
    ) -> Option<Arc<Signature>> {
        let signature = self.class_method(descriptor, "__get__")?;
        if signature.overloads.is_empty() {
            return Some(signature);
        }
        let arguments = self.descriptor_arguments(receiver);
        signature
            .overloads
            .iter()
            .find(|variant| {
                variant
                    .parameters
                    .iter()
                    .zip(arguments)
                    .all(|(parameter, actual)| {
                        parameter
                            .ty
                            .is_none_or(|expected| self.accepts(expected, actual))
                    })
            })
            .cloned()
            .or_else(|| signature.overloads.first().cloned())
    }

    pub(in crate::infrastructure::mypy) fn descriptor_arguments(
        &self,
        receiver: Type,
    ) -> [Type; 2] {
        if self.is_class_value(receiver) {
            [Type::None, receiver]
        } else {
            [receiver, self.class_value_type(receiver)]
        }
    }

    pub(in crate::infrastructure::mypy) fn descriptor_read_type(
        &self,
        descriptor: Type,
        receiver: Type,
    ) -> Option<Type> {
        if let Some(Node::Union(items)) = self.node(descriptor).as_deref() {
            return Some(
                self.union(
                    items
                        .iter()
                        .map(|&item| self.descriptor_read_type(item, receiver).unwrap_or(item))
                        .collect(),
                ),
            );
        }
        let signature = self.descriptor_get_signature(descriptor, receiver)?;
        let positional = signature
            .parameters
            .iter()
            .filter(|parameter| {
                matches!(
                    parameter.kind,
                    super::super::signatures::Kind::PositionalOnly
                        | super::super::signatures::Kind::PositionalOrKeyword
                )
            })
            .collect::<Vec<_>>();
        let variadic = signature
            .parameters
            .iter()
            .find(|parameter| parameter.kind == super::super::signatures::Kind::VariadicPositional);
        let pairs = self
            .descriptor_arguments(receiver)
            .into_iter()
            .enumerate()
            .filter_map(|(index, actual)| {
                Some((positional.get(index).copied().or(variadic)?.ty?, actual))
            })
            .collect::<Vec<_>>();
        let solution = self.solve_call(&signature.generic, &pairs, None);
        signature
            .returns
            .map(|returns| self.substitute_inferred_alias(returns, &solution.types))
    }

    pub(super) fn descriptor_write_type(&self, descriptor: Type, receiver: Type) -> Option<Type> {
        if let Some(Node::Union(items)) = self.node(descriptor).as_deref() {
            return Some(
                self.union(
                    items
                        .iter()
                        .map(|&item| self.descriptor_write_type(item, receiver).unwrap_or(item))
                        .collect(),
                ),
            );
        }
        self.class_method(descriptor, "__set__")
            .and_then(|signature| {
                signature
                    .parameters
                    .get(1)
                    .and_then(|parameter| parameter.ty)
            })
            .or_else(|| self.descriptor_read_type(descriptor, receiver))
    }

    pub(in crate::infrastructure::mypy) fn dataclass_descriptor_input(
        &self,
        descriptor: Type,
    ) -> Result<Type, String> {
        let Some(owner) = self.class_member_owner(descriptor, "__set__") else {
            return Ok(descriptor);
        };
        let class = self.class(owner);
        let name = self.class(self.class_id(descriptor).unwrap()).name.clone();
        let Some(method) = class.methods.get("__set__").filter(|method| {
            method.kind == MethodKind::Instance && method.signature.overloads.is_empty()
        }) else {
            return Err(format!("Unsupported \"__set__\" in \"{name}\""));
        };
        let signature = self.specialize_method(method, descriptor, owner).signature;
        if signature.parameters.len() != 3
            || signature.parameters.iter().any(|parameter| {
                !parameter.required
                    || !matches!(
                        parameter.kind,
                        super::super::signatures::Kind::PositionalOnly
                            | super::super::signatures::Kind::PositionalOrKeyword
                    )
            })
        {
            return Err(format!(
                "Unsupported signature for \"__set__\" in \"{name}\""
            ));
        }
        Ok(signature.parameters[2].ty.unwrap_or(Type::Any))
    }
}
