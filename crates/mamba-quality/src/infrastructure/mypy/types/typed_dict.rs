//! Typed dictionary methods retain fallback signatures and field-specific call results.
use super::super::signatures::{Kind, Parameter, Signature};
use super::*;
use mamba_language::application::quality_syntax::python_ast::ExprCall;
use std::sync::Arc;

impl Context {
    pub(in crate::infrastructure::mypy) fn typed_dict_method(
        &self,
        ty: Type,
        name: &str,
    ) -> Option<Arc<Signature>> {
        if self.is_class_value(ty) {
            return None;
        }
        let key = (self.class_id(ty)?, name.to_string());
        if let Some(signature) = self.classes.borrow().typed_dict_methods.get(&key).cloned() {
            return Some(signature);
        }
        let (class, _) = self.typed_dict(ty)?;
        let (returns, arguments) = match name {
            "copy" => (class.instance, Vec::new()),
            "keys" => (
                self.intern(Node::Container(
                    Container::DictKeys,
                    vec![Type::Str, Type::Object],
                )),
                Vec::new(),
            ),
            "values" => (
                self.intern(Node::Container(
                    Container::DictValues,
                    vec![Type::Str, Type::Object],
                )),
                Vec::new(),
            ),
            "items" => (
                self.intern(Node::Container(
                    Container::DictItems,
                    vec![Type::Str, Type::Object],
                )),
                Vec::new(),
            ),
            "get" => (
                Type::Object,
                vec![
                    ("key", Type::Str, true, Kind::PositionalOnly),
                    ("default", Type::Object, false, Kind::PositionalOnly),
                ],
            ),
            "pop" => (
                Type::Object,
                vec![
                    ("k", Type::Never, true, Kind::PositionalOrKeyword),
                    ("default", Type::Any, false, Kind::PositionalOrKeyword),
                ],
            ),
            "setdefault" => (
                Type::Object,
                vec![
                    ("k", Type::Never, true, Kind::PositionalOrKeyword),
                    ("default", Type::Object, true, Kind::PositionalOrKeyword),
                ],
            ),
            "update" => (
                Type::None,
                vec![("m", class.instance, true, Kind::PositionalOnly)],
            ),
            "__or__" | "__ror__" | "__ior__" => (
                class.instance,
                vec![("value", class.instance, true, Kind::PositionalOnly)],
            ),
            _ => return None,
        };
        let mut signature = Signature::empty(name.into(), returns);
        signature.constructor = false;
        signature.owner = Some(
            if name == "get" {
                "Mapping"
            } else {
                "TypedDict"
            }
            .into(),
        );
        signature.bound_receiver = Some("self".into());
        signature.parameters = arguments
            .into_iter()
            .map(|(name, ty, required, kind)| Parameter {
                name: name.into(),
                ty: Some(ty),
                required,
                kind,
            })
            .collect::<Vec<_>>()
            .into();
        if name == "pop" {
            let variable = self.variable("_T".into(), Type::Object, Vec::new(), None);
            let mut parameters = signature.parameters.to_vec();
            parameters[1].ty = Some(variable);
            signature.parameters = parameters.into();
            signature.generic = Arc::from([variable]);
        }
        if name == "get" {
            signature = self.typed_dict_get_variants(&signature, None);
        }
        if matches!(name, "__or__" | "__ror__") {
            let typed = Arc::new(signature.clone());
            let mut dictionary = signature.clone();
            let mut parameters = dictionary.parameters.to_vec();
            parameters[0].ty =
                Some(self.intern(Node::Container(Container::Dict, vec![Type::Str, Type::Any])));
            dictionary.parameters = parameters.into();
            dictionary.returns = Some(self.intern(Node::Container(
                Container::Dict,
                vec![Type::Str, Type::Object],
            )));
            signature.overloads = Arc::from([typed, Arc::new(dictionary)]);
        }
        let signature = Arc::new(signature);
        self.classes
            .borrow_mut()
            .typed_dict_methods
            .insert(key, signature.clone());
        Some(signature)
    }

    fn typed_dict_get_variants(&self, root: &Signature, contextual: Option<Type>) -> Signature {
        let variable = self.variable("_T".into(), Type::Object, Vec::new(), None);
        let contextual = contextual
            .map(|ty| self.union(vec![ty, variable]))
            .unwrap_or(variable);
        let mut variants = Vec::new();
        for (index, arguments) in [
            vec![Type::Str],
            vec![Type::Str, Type::Object],
            vec![Type::Str, contextual],
        ]
        .into_iter()
        .enumerate()
        {
            let mut signature = root.clone();
            signature.overloads = Arc::from([]);
            signature.parameters = arguments
                .into_iter()
                .map(|ty| Parameter {
                    name: String::new(),
                    ty: Some(ty),
                    kind: Kind::PositionalOnly,
                    required: true,
                })
                .collect::<Vec<_>>()
                .into();
            if index == 2 {
                signature.generic = Arc::from([variable]);
            }
            variants.push(Arc::new(signature));
        }
        let mut signature = root.clone();
        signature.overloads = variants.into();
        signature
    }

    pub(in crate::infrastructure::mypy) fn typed_dict_get_signature(
        &self,
        ty: Type,
        contextual: Option<Type>,
    ) -> Option<Signature> {
        let signature = self.typed_dict_method(ty, "get")?;
        Some(self.typed_dict_get_variants(&signature, contextual))
    }

    fn partial_typed_dict(&self, ty: Type) -> Option<Type> {
        let (original, mut definition) = self.typed_dict(ty)?;
        for item in &mut definition.items {
            item.required = false;
        }
        let id = self.declare_class(
            &format!("<partial:{}:{}>", original.namespace, original.offset),
            &original.name,
            original.offset,
        );
        let identity = self.class(id);
        let mut class = (*original).clone();
        class.instance = identity.instance;
        class.object = identity.object;
        class.typed_dict = Some(definition);
        self.replace_class(id, class);
        Some(identity.instance)
    }

    pub(in crate::infrastructure::mypy) fn typed_dict_call(
        &self,
        call: &ExprCall,
        environment: &VariableMap,
        callback: &impl Fn(&Expr) -> Option<Type>,
    ) -> Option<Type> {
        let Expr::Attribute(attribute) = call.func.as_ref() else {
            return None;
        };
        let receiver = self.infer_with_calls(&attribute.value, environment, callback)?;
        if self.is_class_value(receiver) {
            return None;
        }
        let (_, definition) = self.typed_dict(receiver)?;
        let name = attribute.attr.as_str();
        if !matches!(name, "get" | "pop" | "setdefault") {
            return None;
        }
        let key = call.arguments.args.first().or_else(|| {
            call.arguments
                .keywords
                .iter()
                .find(|keyword| {
                    keyword
                        .arg
                        .as_ref()
                        .is_some_and(|name| name.as_str() == "k")
                })
                .map(|keyword| &keyword.value)
        });
        let Some(key) = key else {
            return Some(Type::Any);
        };
        if name == "get"
            && self
                .infer_with_calls(key, environment, callback)
                .is_some_and(|actual| !self.accepts(Type::Str, actual))
        {
            return Some(Type::Any);
        }
        let keys = self.typed_dict_keys(key, environment);
        let Some(keys) = keys else {
            return Some(if name == "get" {
                Type::Object
            } else {
                Type::Any
            });
        };
        let default = call.arguments.args.get(1).or_else(|| {
            call.arguments
                .keywords
                .iter()
                .find(|keyword| {
                    keyword
                        .arg
                        .as_ref()
                        .is_some_and(|name| name.as_str() == "default")
                })
                .map(|keyword| &keyword.value)
        });
        if call.arguments.args.len() > 2 || (name == "get" && !call.arguments.keywords.is_empty()) {
            return Some(Type::Any);
        }
        if name == "setdefault" && default.is_none() {
            return Some(Type::Object);
        }
        let default_type = default
            .and_then(|value| self.infer_with_calls(value, environment, callback))
            .unwrap_or(Type::None);
        let mut returns = Vec::new();
        for key in keys {
            let Some(item) = definition.items.iter().find(|item| item.name == key) else {
                return Some(if name == "get" {
                    Type::Object
                } else {
                    Type::Any
                });
            };
            if name == "get" && !item.required {
                if matches!(default, Some(Expr::Dict(dictionary)) if dictionary.items.is_empty()) {
                    if let Some(partial) = self.partial_typed_dict(item.ty) {
                        returns.push(partial);
                        continue;
                    }
                }
                let empty_default = match (default, self.node(item.ty)) {
                    (Some(Expr::List(list)), Some(node)) if list.elts.is_empty() => {
                        matches!(node.as_ref(), Node::Container(Container::List, _))
                    }
                    (Some(Expr::Dict(dictionary)), Some(node)) if dictionary.items.is_empty() => {
                        matches!(node.as_ref(), Node::Container(Container::Dict, _))
                    }
                    _ => false,
                };
                if empty_default {
                    returns.push(item.ty);
                    continue;
                }
            }
            returns.push(item.ty);
            if name == "get" && !item.required || name == "pop" && default.is_some() {
                returns.push(default_type);
            }
        }
        Some(self.union(returns))
    }

    pub(in crate::infrastructure::mypy) fn iterable_item(&self, ty: Type) -> Option<Type> {
        if ty.is_any() {
            return Some(Type::Any);
        }
        if !self.is_class_value(ty) && self.typed_dict(ty).is_some() {
            return Some(Type::Str);
        }
        if let Some(node) = self.node(ty) {
            match node.as_ref() {
                Node::Union(items) => {
                    return items
                        .iter()
                        .map(|&item| self.iterable_item(item))
                        .collect::<Option<Vec<_>>>()
                        .map(|items| self.union(items));
                }
                Node::Container(
                    Container::Iterable
                    | Container::Iterator
                    | Container::Generator
                    | Container::Set
                    | Container::FrozenSet
                    | Container::Dict
                    | Container::Mapping
                    | Container::DictKeys,
                    args,
                ) => return Some(args[0]),
                Node::Container(Container::DictValues, args) => return Some(args[1]),
                Node::Container(Container::DictItems, args) => {
                    return Some(self.intern(Node::Tuple(args.clone())))
                }
                _ => {}
            }
        }
        self.sequence_item(ty).or_else(|| {
            (!self.is_class_value(ty))
                .then(|| self.class_method(ty, "__iter__"))?
                .and_then(|signature| signature.returns)
                .and_then(|iterator| self.iterator_item(iterator))
        })
    }

    pub(in crate::infrastructure::mypy) fn iterator_item(&self, ty: Type) -> Option<Type> {
        if ty.is_any() {
            return Some(Type::Any);
        }
        match self.node(ty).as_deref() {
            Some(Node::Container(Container::Iterator | Container::Generator, items)) => {
                Some(items[0])
            }
            _ if !self.is_class_value(ty) => self
                .class_method(ty, "__next__")
                .and_then(|signature| signature.returns),
            _ => None,
        }
    }

    // Only complete single-base source chains reach the locked native dict owner.
    // This does not admit arbitrary nominal Mapping implementations.
    pub(in crate::infrastructure::mypy) fn nominal_dict_base(
        &self,
        receiver: Type,
        target: mamba_language::application::quality_syntax::PythonTarget,
    ) -> Option<(u32, [Type; 2])> {
        let root = self.class_id(receiver)?;
        let slot = match target {
            mamba_language::application::quality_syntax::PythonTarget::Py312 => 0,
            mamba_language::application::quality_syntax::PythonTarget::Py313 => 1,
            mamba_language::application::quality_syntax::PythonTarget::Py314 => 2,
        };
        let libraries = self.classes.borrow().builtin_value_libraries;
        if libraries
            .iter()
            .enumerate()
            .any(|(index, &loaded)| loaded && index != slot)
        {
            return None;
        }
        let native = self.runtime_builtin_class_instance("dict", target)?;
        let libraries = self.classes.borrow().builtin_value_libraries;
        if !libraries[slot]
            || libraries
                .iter()
                .enumerate()
                .any(|(index, &loaded)| loaded && index != slot)
        {
            return None;
        }
        let owner = self.class_id(native)?;
        if self.classes.borrow().keys.get(&("builtins.dict".into(), 0)) != Some(&owner)
            || self.class(owner).generic.len() != 2
        {
            return None;
        }
        let lineage = self.class_mro(root).ok()?;
        for ancestor in lineage {
            self.class_mro(ancestor).ok()?;
        }
        let mut current = receiver;
        let mut active = std::collections::BTreeSet::new();
        loop {
            let (id, arguments) = match self.node(current).as_deref() {
                Some(Node::AppliedClass {
                    id,
                    arguments,
                    value: false,
                }) => (*id, arguments.clone()),
                Some(Node::Instance(id)) if self.class(*id).generic.is_empty() => (*id, Vec::new()),
                _ => return None,
            };
            let class = self.class(id);
            if !active.insert(id) || arguments.len() != class.generic.len() {
                return None;
            }
            if id == owner {
                let inherited = self.class_arguments_for(receiver, owner)?;
                if inherited != arguments {
                    return None;
                }
                return Some((owner, inherited.try_into().ok()?));
            }
            if self.classes.borrow().populated_class_bases.get(&id) != Some(&true)
                || self.classes.borrow().module_targets.get(&class.namespace) != Some(&target)
                || class.plugin
                || class.protocol
                || class.typed_dict.is_some()
                || !class.any_base_origins.is_empty()
                || !class.scalar_bases.is_empty()
            {
                return None;
            }
            let ([parent], [base]) = (class.bases.as_slice(), class.base_types.as_slice()) else {
                return None;
            };
            if self.class_id(*base) != Some(*parent) {
                return None;
            }
            let solutions = class.generic.iter().copied().zip(arguments).collect();
            current = self.substitute(*base, &solutions);
        }
    }

    pub(in crate::infrastructure::mypy) fn mapping_types(&self, ty: Type) -> Option<(Type, Type)> {
        if !self.is_class_value(ty) && self.typed_dict(ty).is_some() {
            return Some((Type::Str, Type::Object));
        }
        let node = self.node(ty)?;
        if let Node::Container(Container::Dict | Container::Mapping, args) = node.as_ref() {
            Some((args[0], args[1]))
        } else {
            None
        }
    }
}
