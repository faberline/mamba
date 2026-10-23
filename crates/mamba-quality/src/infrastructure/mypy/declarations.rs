//! Locked library declarations share structural types with project declarations.
use crate::infrastructure::mypy::{
    signatures::{Kind, Parameter, Signature},
    types::{Container, Context, Literal, Node, ParameterVariance, Type},
};
use serde_json::Value;
use std::{collections::BTreeMap, sync::Arc};
mod parameter_types;

pub(in crate::infrastructure::mypy) struct Reader<'a> {
    pub(in crate::infrastructure::mypy) context: &'a Context,
    pub(in crate::infrastructure::mypy) aliases: BTreeMap<String, u32>,
    pub(in crate::infrastructure::mypy) original_aliases: BTreeMap<String, Value>,
    pub(in crate::infrastructure::mypy) variables: BTreeMap<String, Type>,
    pub(in crate::infrastructure::mypy) aliases_ready: bool,
}

impl Reader<'_> {
    fn declared_type(&mut self, value: &Value) -> Type {
        if value[".class"] == "UnionType" {
            let items = value["items"]
                .as_array()
                .expect("declared union members")
                .iter()
                .map(|item| self.declared_type(item))
                .collect();
            return self.context.intern(Node::Union(items));
        }
        if value[".class"] == "TypeAliasType" {
            let name = value["type_ref"].as_str().expect("declared alias name");
            if !self.context.recursive_alias(self.aliases[name]) {
                if let Some(original) = self.original_aliases.get(name).cloned() {
                    return self.declared_type(&original);
                }
            }
        }
        self.ty(value)
    }
    pub(in crate::infrastructure::mypy) fn alias_target(
        &mut self,
        name: &str,
        value: &Value,
    ) -> Type {
        if value[".class"] != "TypedDictType" {
            return self.ty(value);
        }
        let short = name.rsplit('.').next().expect("typed dictionary name");
        let id = self.context.declare_class(name, short, 0);
        let mut class = (*self.context.class(id)).clone();
        class.typed_dict = Some(
            crate::infrastructure::mypy::classes::typed_dict::Definition {
                closed: value["is_closed"]
                    .as_bool()
                    .expect("typed dictionary closure"),
                items: value["items"]
                    .as_array()
                    .expect("typed dictionary fields")
                    .iter()
                    .map(|item| {
                        let name = item[0].as_str().expect("typed dictionary key");
                        let contains = |key: &str| {
                            value[key]
                                .as_array()
                                .expect("typed dictionary flags")
                                .iter()
                                .any(|item| item.as_str() == Some(name))
                        };
                        crate::infrastructure::mypy::classes::typed_dict::Item {
                            name: name.into(),
                            ty: self.declared_type(&item[1]),
                            required: contains("required_keys"),
                            readonly: contains("readonly_keys"),
                        }
                    })
                    .collect(),
            },
        );
        self.context.replace_class(id, class);
        self.context
            .classes
            .borrow_mut()
            .embedded_typed_dict_closed
            .insert(
                id,
                value["is_closed"]
                    .as_bool()
                    .expect("typed dictionary closure"),
            );
        self.context.class(id).instance
    }

    fn named(&self, name: &str, arguments: Vec<Type>) -> Type {
        if name == "builtins.tuple" && arguments.len() == 1 {
            return self.context.intern(Node::VariadicTuple(arguments[0]));
        }
        if let Some(scalar) = name
            .strip_prefix("builtins.")
            .and_then(Context::builtin_name)
        {
            return scalar;
        }
        let container = match name {
            "builtins.list" => Some(Container::List),
            "builtins.set" => Some(Container::Set),
            "builtins.frozenset" => Some(Container::FrozenSet),
            "builtins.dict" => Some(Container::Dict),
            "typing.Sequence" => Some(Container::Sequence),
            "typing.Mapping" => Some(Container::Mapping),
            "typing.Generator" => Some(Container::Generator),
            "_collections_abc.dict_keys" => Some(Container::DictKeys),
            "_collections_abc.dict_values" => Some(Container::DictValues),
            "_collections_abc.dict_items" => Some(Container::DictItems),
            _ => None,
        };
        if let Some(kind) = container {
            return self.context.intern(Node::Container(kind, arguments));
        }
        let short = name.rsplit('.').next().expect("qualified type has a name");
        let id = self.context.declare_class(name, short, 0);
        if arguments.is_empty() {
            self.context.class(id).instance
        } else {
            self.context.applied_class(id, arguments, false)
        }
    }

    // Only a base position can use the nominal view of a structural annotation.
    pub(in crate::infrastructure::mypy) fn base_type(&mut self, value: &Value) -> Type {
        if value[".class"] == "Instance" {
            let name = value["type_ref"].as_str().expect("base type reference");
            let id = self
                .context
                .classes
                .borrow()
                .keys
                .get(&(name.into(), 0))
                .copied();
            if let Some(id) = id {
                let arguments = self.types(&value["args"]);
                return self.context.applied_class(id, arguments, false);
            }
        }
        self.ty(value)
    }

    fn types(&mut self, values: &Value) -> Vec<Type> {
        values
            .as_array()
            .expect("locked type list")
            .iter()
            .map(|value| self.ty(value))
            .collect()
    }

    pub(in crate::infrastructure::mypy) fn ty(&mut self, value: &Value) -> Type {
        if let Some(name) = value.as_str() {
            return self.named(name, Vec::new());
        }
        match value[".class"].as_str().expect("locked type kind") {
            "UninhabitedType" => Type::Never,
            "ParamSpecType" => self.parameter_spec_type(value),
            "Parameters" => self.parameter_list_type(value),
            "AnyType" => Type::Any,
            "NoneType" => Type::None,
            "TypeType" => {
                let item = self.ty(&value["item"]);
                self.context
                    .intern(Node::Container(Container::TypeObject, vec![item]))
            }
            "UnionType" => {
                let items = self.types(&value["items"]);
                self.context.union(items)
            }
            "TupleType" => {
                let items = self.types(&value["items"]);
                self.context.normalize_tuple(items)
            }
            "Instance" => {
                let args = self.types(&value["args"]);
                self.named(value["type_ref"].as_str().expect("type reference"), args)
            }
            "TypeAliasType" => {
                let id = self.aliases[value["type_ref"].as_str().expect("alias reference")];
                let args = self.types(&value["args"]);
                let instance = self.context.intern(Node::AliasInstance(id, args));
                if self.aliases_ready && !self.context.recursive_alias(id) {
                    self.context.expanded_alias_instance(instance)
                } else {
                    instance
                }
            }
            "TypeVarType" => {
                let key = format!(
                    "{}:{}:{}",
                    value["fullname"], value["namespace"], value["id"]
                );
                if let Some(&ty) = self.variables.get(&key) {
                    return ty;
                }
                let bound = self.ty(&value["upper_bound"]);
                let values = self.types(&value["values"]);
                // mypy's from_omitted_generics marker means the variable has no default.
                let default = if value["default"][".class"] == "AnyType"
                    && value["default"]["type_of_any"] == 4
                {
                    None
                } else {
                    Some(self.ty(&value["default"]))
                };
                let ty = self.context.variable_with_variance(
                    value["name"].as_str().expect("variable name").into(),
                    bound,
                    values,
                    default,
                    match value["variance"].as_u64().expect("variable variance") {
                        0 => ParameterVariance::Invariant,
                        1 => ParameterVariance::Covariant,
                        2 => ParameterVariance::Contravariant,
                        3 => ParameterVariance::Inferred,
                        code => panic!("unsupported locked variance: {code}"),
                    },
                );
                self.variables.insert(key, ty);
                ty
            }
            "LiteralType" => {
                let literal = if let Some(text) = value["value"].as_str() {
                    Literal::Str(text.into())
                } else if let Some(flag) = value["value"].as_bool() {
                    Literal::Bool(flag)
                } else {
                    Literal::Int(value["value"].as_i64().expect("integer literal"))
                };
                self.context.intern(Node::Literal(literal))
            }
            "CallableType" | "Overloaded" => {
                let signature = self.signature(value);
                self.context.callable_type(Arc::new(signature))
            }
            "TypedDictType" => {
                let fallback = value["fallback"]
                    .as_str()
                    .or_else(|| value["fallback"]["type_ref"].as_str())
                    .expect("named typed dictionary fallback");
                self.alias_target(fallback, value)
            }
            kind => panic!("unsupported locked Pydantic type kind: {kind}"),
        }
    }

    pub(in crate::infrastructure::mypy) fn decorated_signature(
        &mut self,
        value: &Value,
    ) -> Signature {
        let mut signature = self.signature(value);
        signature.return_explicit = false;
        for parameter in Arc::make_mut(&mut signature.parameters) {
            parameter.ty = parameter.ty.map(|ty| self.decorated_parameter(ty));
        }
        signature
    }

    fn decorated_parameter(&self, ty: Type) -> Type {
        let Some(node) = self.context.node(ty) else {
            return ty;
        };
        if let Node::Union(items) = node.as_ref() {
            self.context.union(
                items
                    .iter()
                    .map(|&item| self.context.expanded_alias_instance(item))
                    .collect(),
            )
        } else {
            ty
        }
    }

    pub(in crate::infrastructure::mypy) fn signature(&mut self, value: &Value) -> Signature {
        if value[".class"] == "Overloaded" {
            let variants: Vec<_> = value["items"]
                .as_array()
                .expect("locked overload variants")
                .iter()
                .map(|value| Arc::new(self.signature(value)))
                .collect();
            let mut signature = (**variants.last().expect("nonempty overloads")).clone();
            signature.overloads = variants.into();
            return signature;
        }
        let returns = self.ty(&value["ret_type"]);
        let mut signature = Signature::empty("Field".into(), returns);
        signature.name = value["name"].as_str().map(|_| "Field".into());
        signature.native_symbol = signature.name.as_ref().map(|_| "pydantic.fields.Field");
        signature.constructor = false;
        signature.definition_module = Some("pydantic.fields".into());
        signature.generic = self.types(&value["variables"]).into();
        for (key, kind) in [
            (
                "type_guard",
                crate::infrastructure::mypy::signatures::NarrowingKind::TypeGuard,
            ),
            (
                "type_is",
                crate::infrastructure::mypy::signatures::NarrowingKind::TypeIs,
            ),
        ] {
            if let Some(ty) = value.get(key).filter(|ty| !ty.is_null()) {
                signature.narrowing = Some((kind, self.ty(ty)));
            }
        }
        signature.ellipsis = value["is_ellipsis_args"].as_bool().expect("callable mode");
        signature.parameters = value["arg_types"]
            .as_array()
            .expect("parameter types")
            .iter()
            .enumerate()
            .map(|(index, value_type)| {
                let code = value["arg_kinds"][index].as_u64().expect("parameter kind");
                let name = value["arg_names"][index].as_str().unwrap_or_default();
                // Keep keyword eligibility from arg_names/kind, but retain the
                // original positional-only receiver spelling for later binding.
                let original_name =
                    (index == 0 && value["arg_names"][index].is_null() && matches!(code, 0 | 1))
                        .then(|| value["mamba_original_first_arg"].as_str())
                        .flatten()
                        .filter(|name| !name.is_empty());
                Parameter {
                    name: original_name.unwrap_or(name).into(),
                    ty: Some(self.ty(value_type)),
                    kind: match code {
                        0 | 1 if name.is_empty() => Kind::PositionalOnly,
                        0 | 1 => Kind::PositionalOrKeyword,
                        2 => Kind::VariadicPositional,
                        3 | 5 => Kind::KeywordOnly,
                        4 => Kind::VariadicKeyword,
                        _ => panic!("unsupported locked parameter kind: {code}"),
                    },
                    required: matches!(code, 0 | 3),
                }
            })
            .collect::<Vec<_>>()
            .into();
        signature
    }
}
