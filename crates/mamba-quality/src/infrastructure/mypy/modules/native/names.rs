//! Runtime Any is a class constructor; annotation Any remains a dynamic type.
use super::*;
use crate::infrastructure::mypy::{
    signatures::{Kind, Parameter, Signature},
    types::{Container, Node},
};
use std::sync::Arc;

impl Lookup {
    pub(super) fn name_member(&self, module: &str, member: &str) -> Option<Export> {
        if !matches!(module, "typing" | "typing_extensions" | "abc") {
            return None;
        }
        if member == "Literal" && matches!(module, "typing" | "typing_extensions") {
            let marker = self.modules.context.intern(Node::LiteralForm);
            return Some(Export {
                annotation_module: false,
                ty: Some(Type::Any),
                callable: None,
                module: None,
                explicit: true,
                module_hidden: Some(false),
                alias: Some(marker),
            });
        }
        if member == "Self" && matches!(module, "typing" | "typing_extensions") {
            let marker = self.modules.context.intern(Node::TypingForm("Self".into()));
            return Some(Export {
                annotation_module: false,
                ty: Some(Type::Any),
                callable: None,
                module: None,
                explicit: true,
                module_hidden: Some(false),
                alias: Some(marker),
            });
        }
        let native = match (module, member) {
            ("typing" | "typing_extensions", "dataclass_transform") => {
                Some("typing.dataclass_transform")
            }
            ("typing" | "typing_extensions", "final") => Some("typing.final"),
            ("typing" | "typing_extensions", "override") => Some("typing.override"),
            ("abc", "abstractmethod") => Some("abc.abstractmethod"),
            _ => None,
        };
        if let Some(native) = native {
            let context = &self.modules.context;
            // Keep the same Callable[..., Any] shape as the internal helper.
            let mut callable = Signature::empty(String::new(), Type::Any);
            callable.name = None;
            callable.constructor = false;
            callable.ellipsis = true;
            callable.parameters = [Kind::VariadicPositional, Kind::VariadicKeyword]
                .into_iter()
                .map(|kind| Parameter {
                    name: String::new(),
                    ty: Some(Type::Any),
                    kind,
                    required: false,
                })
                .collect::<Vec<_>>()
                .into();
            let callable = context.callable_type(Arc::new(callable));
            let bound = if matches!(native, "typing.override" | "abc.abstractmethod") {
                callable
            } else {
                Type::Object
            };
            let variable = context.native_variable(native, 0, "_T".into(), bound);
            let parameter = |name: &str, ty, kind, required| Parameter {
                name: name.into(),
                ty: Some(ty),
                kind,
                required,
            };
            let mut identity = Signature::empty(member.into(), variable);
            identity.constructor = false;
            identity.generic = vec![variable].into();
            identity.parameters = vec![parameter(
                match native {
                    "typing.final" => "f",
                    "typing.override" => "method",
                    "abc.abstractmethod" => "funcobj",
                    _ => "x",
                },
                variable,
                if matches!(native, "typing.final" | "abc.abstractmethod") {
                    Kind::PositionalOrKeyword
                } else {
                    Kind::PositionalOnly
                },
                true,
            )]
            .into();
            let mut signature = if native == "typing.dataclass_transform" {
                // Typeshed's IdentityFunction has a generic __call__(x: T) -> T.
                let returns = context.callable_type(Arc::new(identity));
                let mut factory = Signature::empty(member.into(), returns);
                factory.constructor = false;
                let type_object =
                    context.intern(Node::Container(Container::TypeObject, vec![Type::Any]));
                let fields = context.intern(Node::VariadicTuple(
                    context.union(vec![type_object, callable]),
                ));
                factory.parameters = [
                    parameter("eq_default", Type::Bool, Kind::KeywordOnly, false),
                    parameter("order_default", Type::Bool, Kind::KeywordOnly, false),
                    parameter("kw_only_default", Type::Bool, Kind::KeywordOnly, false),
                    parameter("frozen_default", Type::Bool, Kind::KeywordOnly, false),
                    parameter("field_specifiers", fields, Kind::KeywordOnly, false),
                    parameter("kwargs", Type::Any, Kind::VariadicKeyword, false),
                ]
                .into();
                factory
            } else {
                identity
            };
            signature.native_symbol = Some(native);
            signature.definition_module = Some(module.into());
            let signature = Arc::new(signature);
            return Some(Export {
                annotation_module: false,
                ty: Some(context.callable_type(signature.clone())),
                callable: Some(signature),
                module: None,
                explicit: true,
                module_hidden: Some(false),
                alias: None,
            });
        }
        if module == "abc" {
            return None;
        }
        if member == "cast" {
            let context = &self.modules.context;
            let variable = context.native_variable("typing.cast", 0, "_T".into(), Type::Object);
            let type_object =
                context.intern(Node::Container(Container::TypeObject, vec![variable]));
            let parameter = |name: &str, ty| Parameter {
                name: name.into(),
                ty: Some(ty),
                kind: Kind::PositionalOrKeyword,
                required: true,
            };
            let variant = |typ, returns, generic: Vec<Type>| {
                let mut signature = Signature::empty("cast".into(), returns);
                signature.constructor = false;
                signature.parameters = [parameter("typ", typ), parameter("val", Type::Any)].into();
                signature.generic = generic.into();
                Arc::new(signature)
            };
            let mut signature = Signature::empty("cast".into(), Type::Any);
            signature.constructor = false;
            signature.native_symbol = Some("typing.cast");
            signature.definition_module = Some("typing".into());
            signature.overloads = [
                variant(type_object, variable, vec![variable]),
                variant(Type::Str, Type::Any, Vec::new()),
                variant(Type::Object, Type::Any, Vec::new()),
            ]
            .into();
            let signature = Arc::new(signature);
            return Some(Export {
                annotation_module: false,
                ty: Some(context.callable_type(signature.clone())),
                callable: Some(signature),
                module: None,
                explicit: true,
                module_hidden: Some(false),
                alias: None,
            });
        }
        if member != "Any" {
            return None;
        }
        let context = &self.modules.context;
        let ty = context.typing_any_value(context.module_target(&self.namespace()));
        Some(Export {
            annotation_module: false,
            ty: Some(ty),
            callable: context.callable_signature(ty),
            module: None,
            explicit: true,
            module_hidden: Some(false),
            alias: Some(Type::Any),
        })
    }
}
