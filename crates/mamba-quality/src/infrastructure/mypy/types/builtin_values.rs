//! Builtin value inference reads locked declarations without running Python.
use super::*;
use crate::infrastructure::mypy::declarations::Reader;
use mamba_language::application::quality_syntax::PythonTarget;
use serde_json::Value;
use std::sync::{Arc, OnceLock};
mod classes;
mod members;
mod signatures;
mod source_classes;

fn slot(target: PythonTarget) -> usize {
    match target {
        PythonTarget::Py312 => 0,
        PythonTarget::Py313 => 1,
        PythonTarget::Py314 => 2,
    }
}

fn data(target: PythonTarget) -> &'static Value {
    static DATA: OnceLock<Value> = OnceLock::new();
    &DATA.get_or_init(|| {
        serde_json::from_str(include_str!(
            "../../../../assets/stdlib/builtin-values.json"
        ))
        .expect("locked builtin declarations")
    })["targets"][match target {
        PythonTarget::Py312 => "3.12",
        PythonTarget::Py313 => "3.13",
        PythonTarget::Py314 => "3.14",
    }]
}

impl Context {
    // None is unproved; Some(false) proves absence. Callers may first use the
    // existing canonical loader. This proof only reads its completed metadata.
    pub(in crate::infrastructure::mypy) fn builtin_dictionary_member_presence(
        &self,
        declared: Type,
        member: &str,
        target: PythonTarget,
    ) -> Option<bool> {
        use std::collections::BTreeSet;
        if member.starts_with("__") && member.ends_with("__") {
            return None;
        }
        let registry = self.classes.borrow();
        if !registry.builtin_value_libraries[slot(target)]
            || registry
                .builtin_value_libraries
                .iter()
                .filter(|&&ready| ready)
                .count()
                != 1
        {
            return None;
        }
        let classes = data(target)["classes"].as_object()?;
        let node = self.node(declared)?;
        let Node::Instance(root) = node.as_ref() else {
            return None;
        };
        let root = *root;
        if registry.keys.get(&("builtins.dict".into(), 0)) != Some(&root)
            || registry.entries.get(root as usize)?.namespace != "builtins.dict"
        {
            return None;
        }
        let object = *registry.keys.get(&("<builtins>".into(), 0))?;
        let mut complete = BTreeSet::new();
        let mut active = BTreeSet::new();
        let mut pending = vec![(root, false)];
        let mut present = false;
        let mut native_object_edge = false;
        let mut declared_object_edge = false;
        while let Some((owner, finish)) = pending.pop() {
            if finish {
                active.remove(&owner);
                complete.insert(owner);
                continue;
            }
            if complete.contains(&owner) {
                continue;
            }
            if !active.insert(owner) {
                return None;
            }
            let class = registry.entries.get(owner as usize)?;
            let fullname = if owner == object {
                if class.namespace != "<builtins>"
                    || class.offset != 0
                    || registry.populated_class_bases.get(&owner) != Some(&true)
                    || class.scalar_bases.as_slice() != [Type::Object]
                {
                    return None;
                }
                "builtins.object"
            } else {
                if class.offset != 0
                    || registry.keys.get(&(class.namespace.clone(), 0)) != Some(&owner)
                {
                    return None;
                }
                class.namespace.as_str()
            };
            let locked = classes.get(fullname)?;
            // Native leaves may retain Object as a scalar rather than a class
            // edge. Admit only the exact base declared by this target's catalog.
            let compact_object_base = owner != object
                && class.bases.is_empty()
                && class.base_types.is_empty()
                && class.scalar_bases.as_slice() == [Type::Object]
                && locked["bases"] == serde_json::json!(["builtins.object"]);
            if locked["fullname"].as_str()? != fullname
                || (owner != object && !class.scalar_bases.is_empty() && !compact_object_base)
                || class.name != fullname.rsplit('.').next()?
                || class.generic.len() != locked["generic"].as_array()?.len()
                || class.protocol != locked["protocol"].as_bool()?
                || class.runtime_protocol != locked["runtime_protocol"].as_bool()?
                || registry.populated_class_bases.get(&owner) == Some(&false)
                || class.plugin
                || class.typed_dict.is_some()
                || class.dataclass.is_some()
                || class.pydantic.is_some()
                || !class.any_base_origins.is_empty()
                || !class.erased_members.is_empty()
                || !matches!(self.node(class.instance).as_deref(),
                    Some(Node::Instance(id)) if *id == owner)
                || !matches!(self.node(class.object).as_deref(),
                    Some(Node::Class(id)) if *id == owner)
            {
                return None;
            }
            let fields = locked["fields"].as_object()?;
            let methods = locked["methods"].as_object()?;
            // A later replacement may leave the library flag true. Every locked
            // name must still exist, even when its current value type is unknown.
            if !fields
                .keys()
                .chain(methods.keys())
                .all(|name| class.fields.contains_key(name) || class.methods.contains_key(name))
                || class.fields.contains_key("__getattr__")
                || class.methods.contains_key("__getattr__")
                || class.fields.contains_key("__getattribute__")
                || (owner != object && class.methods.contains_key("__getattribute__"))
            {
                return None;
            }
            if owner == object {
                let lookup = methods.get("__getattribute__")?;
                let value = &lookup["type"];
                let actual = class.methods.get("__getattribute__")?;
                let signature = &actual.signature;
                let parameters = signature.parameters.as_ref();
                if lookup["class_method"] != false
                    || lookup["static_method"] != false
                    || lookup["property"] != false
                    || value[".class"] != "CallableType"
                    || value["arg_types"] != serde_json::json!(["builtins.object", "builtins.str"])
                    || value["arg_kinds"] != serde_json::json!([0, 0])
                    || value["arg_names"] != serde_json::json!([null, null])
                    || !value["variables"].as_array()?.is_empty()
                    || value["is_ellipsis_args"] != false
                    || value["ret_type"][".class"] != "AnyType"
                    || value["ret_type"]["type_of_any"] != 2
                    || actual.kind != crate::infrastructure::mypy::classes::MethodKind::Instance
                    || actual.setter.is_some()
                    || signature.definition_module.as_deref() != Some("builtins")
                    || signature.owner.as_deref() != Some("object")
                    || signature.name.as_deref() != Some("__getattribute__")
                    || signature.constructor
                    || signature.ellipsis
                    || signature.implicit
                    || signature.ignored
                    || signature.bound_receiver.is_some()
                    || signature.narrowing.is_some()
                    || !signature.generic.is_empty()
                    || !signature.overloads.is_empty()
                    || signature.returns != Some(Type::Any)
                    || !matches!(parameters, [receiver, name]
                        if receiver.ty == Some(Type::Object) && name.ty == Some(Type::Str)
                            && parameters.iter().all(|parameter|
                                parameter.required && parameter.kind ==
                                    crate::infrastructure::mypy::signatures::Kind::PositionalOnly))
                {
                    return None;
                }
            }
            let bases = locked["bases"].as_array()?;
            if !compact_object_base
                && (bases.len() != class.bases.len() || bases.len() != class.base_types.len())
            {
                return None;
            }
            for ((expected, &parent), &actual) in
                bases.iter().zip(&class.bases).zip(&class.base_types)
            {
                let (name, arguments) = if let Some(name) = expected.as_str() {
                    (name, 0)
                } else if expected[".class"] == "Instance" && expected.get("extra_attrs")?.is_null()
                {
                    (
                        expected["type_ref"].as_str()?,
                        expected["args"].as_array()?.len(),
                    )
                } else {
                    return None;
                };
                let expected_id = if name == "builtins.object" {
                    object
                } else {
                    *registry.keys.get(&(name.into(), 0))?
                };
                if parent != expected_id
                    || !matches!(self.node(actual).as_deref(),
                        Some(Node::Instance(id)) if *id == parent && arguments == 0)
                        && !matches!(self.node(actual).as_deref(),
                            Some(Node::AppliedClass { id, arguments: args, value: false })
                                if *id == parent && args.len() == arguments)
                {
                    return None;
                }
            }
            native_object_edge |= compact_object_base;
            declared_object_edge |= class.bases.contains(&object);
            present |= class.fields.contains_key(member) || class.methods.contains_key(member);
            pending.push((owner, true));
            pending.extend(class.bases.iter().rev().map(|&parent| (parent, false)));
            if compact_object_base {
                // Validate the real Object entry, including its default getter.
                pending.push((object, false));
            }
        }
        drop(registry);
        // MRO queries can update their existing cache, so no registry borrow
        // crosses them. Check every ancestor, not only the root's cached order.
        if !complete.iter().all(|&owner| {
            self.class_mro(owner)
                .is_ok_and(|lineage| owner != object || lineage.as_slice() == [object])
        }) {
            return None;
        }
        let mut lineage = self.class_mro(root).ok()?;
        let mut represented = complete.clone();
        let append_object = native_object_edge && !declared_object_edge;
        if append_object {
            represented.remove(&object);
        }
        // First prove the original MRO covers exactly the represented graph.
        if lineage.first() != Some(&root)
            || lineage.len() != represented.len()
            || lineage.iter().copied().collect::<BTreeSet<_>>() != represented
            || (native_object_edge && declared_object_edge && lineage.last() != Some(&object))
        {
            return None;
        }
        if append_object {
            // Object is the common terminal base of the catalog-verified leaves.
            // This local proof projection does not alter classes or MRO caches.
            lineage.push(object);
        }
        (complete.contains(&object)
            && lineage.first() == Some(&root)
            && lineage.len() == complete.len()
            && lineage.iter().copied().collect::<BTreeSet<_>>() == complete)
            .then_some(present)
    }

    pub(in crate::infrastructure::mypy) fn runtime_builtin_class_instance(
        &self,
        name: &str,
        target: PythonTarget,
    ) -> Option<Type> {
        let namespace = format!("builtins.{name}");
        data(target)["classes"].get(namespace.as_str())?;
        self.load_builtin_value_classes(target);
        let id = Self::builtin_name(name)
            .map(|scalar| self.builtin_class(scalar))
            .or_else(|| self.classes.borrow().keys.get(&(namespace, 0)).copied())?;
        Some(self.class(id).instance)
    }

    pub(in crate::infrastructure::mypy) fn runtime_builtin_value(
        &self,
        name: &str,
        target: PythonTarget,
    ) -> Option<Type> {
        let value = data(target)["values"].get(name)?;
        let key = (slot(target), name.to_string());
        if let Some(&ty) = self.classes.borrow().builtin_values.get(&key) {
            return Some(ty);
        }
        self.load_builtin_value_classes(target);
        let mut reader = self.builtin_value_reader(target);
        let ty = if matches!(
            value[".class"].as_str(),
            Some("CallableType" | "Overloaded")
        ) {
            let signature = signatures::read(&mut reader, value, "builtins", None);
            if let Some(instance) = signatures::instance(value).map(|value| reader.ty(value)) {
                let ty = self.class_value_type(instance);
                self.classes
                    .borrow_mut()
                    .builtin_value_constructors
                    .insert((slot(target), ty), Arc::new(signature));
                ty
            } else {
                self.callable_type(Arc::new(signature))
            }
        } else {
            reader.ty(value)
        };
        self.classes.borrow_mut().builtin_values.insert(key, ty);
        Some(ty)
    }

    pub(in crate::infrastructure::mypy) fn builtin_value_constructor(
        &self,
        ty: Type,
        target: PythonTarget,
    ) -> Option<Arc<crate::infrastructure::mypy::signatures::Signature>> {
        self.classes
            .borrow()
            .builtin_value_constructors
            .get(&(slot(target), ty))
            .cloned()
    }

    pub(in crate::infrastructure::mypy) fn builtin_value_class_names(
        target: PythonTarget,
    ) -> Vec<String> {
        static NAMES: [OnceLock<Vec<String>>; 3] =
            [OnceLock::new(), OnceLock::new(), OnceLock::new()];

        NAMES[slot(target)]
            .get_or_init(|| {
                data(target)["values"]
                    .as_object()
                    .unwrap()
                    .iter()
                    .filter(|(name, value)| {
                        (!name.starts_with('_') || name.starts_with("__"))
                            && signatures::instance(value).is_some()
                    })
                    .map(|(name, _)| name.clone())
                    .collect()
            })
            .clone()
    }

    fn builtin_value_reader(&self, target: PythonTarget) -> Reader<'_> {
        let aliases = data(target)["aliases"]
            .as_object()
            .expect("builtin aliases");
        Reader {
            context: self,
            aliases: self.classes.borrow().builtin_value_aliases[slot(target)].clone(),
            original_aliases: aliases
                .iter()
                .map(|(name, value)| (name.clone(), value["target"].clone()))
                .collect(),
            variables: self.classes.borrow().builtin_value_variables[slot(target)]
                .as_map()
                .clone(),
            aliases_ready: true,
        }
    }
}
