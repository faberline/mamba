//! Callable values use the fixed function member types without executing Python.
use super::super::signatures::{Kind, Parameter};
use super::*;
use std::sync::Arc;

impl Context {
    pub(in crate::infrastructure::mypy) fn callable_attribute(
        &self,
        ty: Type,
        name: &str,
    ) -> Option<Type> {
        if !matches!(self.node(ty).as_deref(), Some(Node::Callable(_))) {
            return None;
        }
        let dictionary =
            || self.intern(Node::Container(Container::Dict, vec![Type::Str, Type::Any]));
        match name {
            "__name__" | "__qualname__" | "__module__" => Some(Type::Str),
            "__doc__" => Some(self.intern(Node::Union(vec![Type::Str, Type::None]))),
            "__annotations__" | "__dict__" | "__globals__" => Some(dictionary()),
            "__defaults__" => Some(self.intern(Node::Union(vec![
                self.intern(Node::VariadicTuple(Type::Any)),
                Type::None,
            ]))),
            "__kwdefaults__" => Some(self.intern(Node::Union(vec![dictionary(), Type::None]))),
            "__code__" => Some(
                self.class(self.declare_class("types", "CodeType", u32::MAX))
                    .instance,
            ),
            "__closure__" => Some(self.intern(Node::Union(vec![
                self.intern(Node::VariadicTuple(
                    self.class(self.declare_class("types", "CellType", u32::MAX - 1)).instance,
                )),
                Type::None,
            ]))),
            "__type_params__" => Some(
                self.intern(Node::VariadicTuple(
                    self.intern(Node::Union(
                        [
                            ("typing", "TypeVar"),
                            ("typing", "ParamSpec"),
                            ("typing_extensions", "TypeVarTuple"),
                        ]
                        .into_iter()
                        .enumerate()
                        .map(|(index, (module, name))| {
                            self.class(self.declare_class(module, name, u32::MAX - index as u32))
                                .instance
                        })
                        .collect(),
                    )),
                )),
            ),
            "__get__" => {
                let mut signature = self
                    .callable_signature(self.bare_callable(&Variables::new()))?
                    .as_ref()
                    .clone();
                let class = self
                    .class(self.declare_class("<builtins>", "type", u32::MAX))
                    .instance;
                signature.parameters = vec![
                    Parameter {
                        name: String::new(),
                        kind: Kind::PositionalOnly,
                        required: true,
                        ty: Some(Type::Object),
                    },
                    Parameter {
                        name: String::new(),
                        kind: Kind::PositionalOnly,
                        required: false,
                        ty: Some(self.intern(Node::Union(vec![class, Type::None]))),
                    },
                ]
                .into();
                signature.ellipsis = false;
                Some(self.callable_type(Arc::new(signature)))
            }
            _ => None,
        }
    }
}
