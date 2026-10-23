//! Conversion protocols retain the method signatures from the locked type data.
use super::super::{
    classes::{Abstractness, Method, MethodKind},
    signatures::{Kind, Parameter, Signature},
};
use super::*;
use std::sync::Arc;

impl Context {
    pub(in crate::infrastructure::mypy) fn conversion_protocol(&self, name: &str) -> Type {
        let (module, offset, method, returned) = match name {
            "SupportsInt" => ("typing", 100, "__int__", Type::Int),
            "SupportsIndex" => ("typing", 101, "__index__", Type::Int),
            "SupportsFloat" => ("typing", 102, "__float__", Type::Float),
            "SupportsComplex" => ("typing", 103, "__complex__", Type::Complex),
            "SupportsBytes" => ("typing", 104, "__bytes__", Type::Bytes),
            "SupportsTrunc" => ("_typeshed", 100, "__trunc__", Type::Int),
            "Buffer" => {
                let view = self.declare_class("builtins.memoryview", "memoryview", 0);
                (
                    "_collections_abc",
                    100,
                    "__buffer__",
                    self.class(view).instance,
                )
            }
            _ => unreachable!("locked conversion protocol name"),
        };
        let id = self.declare_class(module, name, offset);
        let old = self.class(id);
        if old.protocol {
            return old.instance;
        }
        let mut class = (*old).clone();
        class.protocol = true;
        let mut signature = Signature::empty(method.into(), returned);
        signature.constructor = false;
        let mut parameters = vec![Parameter {
            name: "self".into(),
            ty: Some(class.instance),
            kind: Kind::PositionalOrKeyword,
            required: true,
        }];
        if name == "Buffer" {
            parameters.push(Parameter {
                name: "flags".into(),
                ty: Some(Type::Int),
                kind: Kind::PositionalOnly,
                required: true,
            });
        }
        signature.parameters = parameters.into();
        class.methods.insert(
            method.into(),
            Method {
                signature: Arc::new(signature),
                kind: MethodKind::Instance,
                setter: None,
                final_method: false,
                abstractness: Abstractness::Explicit,
            },
        );
        let instance = class.instance;
        self.replace_class(id, class);
        instance
    }

    pub(super) fn builtin_conversion_method(&self, ty: Type, name: &str) -> Option<Arc<Signature>> {
        if let Some(Node::Literal(value) | Node::LiteralValue(value)) = self.node(ty).as_deref() {
            return self.builtin_conversion_method(value.base(), name);
        }
        let returned = match (ty, name) {
            (Type::Int | Type::Bool, "__int__" | "__index__" | "__trunc__")
            | (Type::Float, "__int__" | "__trunc__") => Type::Int,
            (Type::Int | Type::Bool | Type::Float, "__float__") => Type::Float,
            (Type::Complex, "__complex__") => Type::Complex,
            (Type::Bytes, "__bytes__") => Type::Bytes,
            (Type::Bytes, "__buffer__") => {
                self.class(self.declare_class("builtins.memoryview", "memoryview", 0))
                    .instance
            }
            _ => return None,
        };
        let mut signature = Signature::empty(name.into(), returned);
        signature.constructor = false;
        signature.owner = Some(self.name(if ty == Type::Bool { Type::Int } else { ty }));
        signature.bound_receiver = Some("self".into());
        if name == "__buffer__" {
            signature.parameters = vec![Parameter {
                name: "flags".into(),
                ty: Some(Type::Int),
                kind: Kind::PositionalOnly,
                required: true,
            }]
            .into();
        }
        Some(Arc::new(signature))
    }
}
