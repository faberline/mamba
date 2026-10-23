//! Builtin class objects retain their scalar identity and inherited conversion members.
use super::super::{
    classes::{Abstractness, Class, Method, MethodKind},
    signatures::{Kind, Parameter, Signature},
};
use super::*;
use mamba_language::application::quality_syntax::PythonTarget;
use std::sync::Arc;

impl Context {
    pub(in crate::infrastructure::mypy) fn builtin_name(name: &str) -> Option<Type> {
        Some(match name {
            "int" => Type::Int,
            "float" => Type::Float,
            "complex" => Type::Complex,
            "str" => Type::Str,
            "bytes" => Type::Bytes,
            "bool" => Type::Bool,
            "object" => Type::Object,
            _ => return None,
        })
    }

    pub(in crate::infrastructure::mypy) fn builtin_scalar(&self, ty: Type) -> Option<Type> {
        if matches!(
            ty,
            Type::Int
                | Type::Float
                | Type::Complex
                | Type::Str
                | Type::Bytes
                | Type::Bool
                | Type::Object
        ) {
            return Some(ty);
        }
        let class = self.class(self.class_id(ty)?);
        (class.namespace == "<builtins>" && class.offset <= 6)
            .then(|| class.scalar_bases.first().copied())
            .flatten()
    }

    pub(in crate::infrastructure::mypy) fn populate_builtin_members(
        &self,
        class: &mut Class,
        ty: Type,
    ) {
        class.final_class = ty == Type::Bool;
        if ty == Type::Object {
            for name in ["__eq__", "__ne__"] {
                if let Some(signature) = self.object_comparison_method(name) {
                    class.methods.insert(
                        name.into(),
                        Method {
                            signature,
                            kind: MethodKind::Instance,
                            setter: None,
                            final_method: false,
                            abstractness: Abstractness::Concrete,
                        },
                    );
                }
            }
        }
        if let Some(signature) = self.builtin_allocation_member(ty, PythonTarget::Py312) {
            class.methods.insert(
                "__new__".into(),
                Method {
                    signature,
                    kind: MethodKind::Static,
                    setter: None,
                    final_method: false,
                    abstractness: Abstractness::Concrete,
                },
            );
        }
        if ty == Type::Bool {
            return;
        }
        for name in [
            "__int__",
            "__index__",
            "__float__",
            "__complex__",
            "__bytes__",
            "__trunc__",
            "__buffer__",
        ] {
            if let Some(signature) = self.builtin_conversion_method(ty, name) {
                class.methods.insert(
                    name.into(),
                    Method {
                        signature: Arc::new(unbound_signature(&signature, name, ty, &class.name)),
                        kind: MethodKind::Instance,
                        setter: None,
                        final_method: false,
                        abstractness: Abstractness::Concrete,
                    },
                );
            }
        }
    }

    pub(super) fn builtin_allocation_member(
        &self,
        ty: Type,
        target: PythonTarget,
    ) -> Option<Arc<Signature>> {
        let signature = self.builtin_constructor(ty, target)?;
        let previous = self
            .classes
            .borrow()
            .builtin_self_variables
            .get(&ty)
            .copied();
        let variable = previous.unwrap_or_else(|| {
            let variable = self.variable("Self".into(), ty, vec![], None);
            self.classes
                .borrow_mut()
                .builtin_self_variables
                .insert(ty, variable);
            variable
        });
        let receiver = self.intern(Node::Container(Container::TypeObject, vec![variable]));
        let mut allocation = allocation_signature(&signature, receiver, variable, &self.name(ty));
        if ty == Type::Bool {
            allocation.returns = Some(Type::Bool);
        }
        Some(Arc::new(allocation))
    }
}

fn allocation_signature(
    signature: &Signature,
    receiver: Type,
    returned: Type,
    owner: &str,
) -> Signature {
    let mut signature = unbound_signature(signature, "__new__", receiver, owner);
    signature.returns = Some(returned);
    signature.generic = vec![returned].into();
    signature.overloads = signature
        .overloads
        .iter()
        .map(|variant| {
            let mut variant = (**variant).clone();
            variant.returns = Some(returned);
            variant.generic = vec![returned].into();
            Arc::new(variant)
        })
        .collect();
    signature
}

fn unbound_signature(signature: &Signature, name: &str, receiver: Type, owner: &str) -> Signature {
    let positional = if name == "__new__" {
        signature
            .parameters
            .first()
            .is_some_and(|parameter| parameter.kind == Kind::PositionalOnly)
    } else {
        super::super::signatures::special_methods::positional_only(name)
    };
    let first = Parameter {
        name: if name == "__new__" { "cls" } else { "self" }.into(),
        ty: Some(receiver),
        kind: if positional {
            Kind::PositionalOnly
        } else {
            Kind::PositionalOrKeyword
        },
        required: true,
    };
    Signature {
        name: Some(name.into()),
        owner: Some(owner.into()),
        constructor: false,
        bound_receiver: None,
        parameters: std::iter::once(first)
            .chain(signature.parameters.iter().cloned())
            .collect::<Vec<_>>()
            .into(),
        overloads: signature
            .overloads
            .iter()
            .map(|variant| Arc::new(unbound_signature(variant, name, receiver, owner)))
            .collect(),
        ..signature.clone()
    }
}
