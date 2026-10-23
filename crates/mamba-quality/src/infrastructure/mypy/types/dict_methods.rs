//! Dictionary get overloads retain the locked builtins stub's parameter types.
use super::super::signatures::{Kind, Parameter, Signature};
use super::*;
use mamba_language::application::quality_syntax::PythonTarget;
use std::sync::Arc;

impl Context {
    pub(super) fn nominal_dict_method(
        &self,
        ty: Type,
        name: &str,
        target: PythonTarget,
    ) -> Option<Arc<Signature>> {
        let receiver = self.nominal_dict_instance(ty, target)?;
        self.class_method_for(receiver, name, target)
    }

    // Class bases and member lookup share the same locked nominal dict owner.
    pub(in crate::infrastructure::mypy) fn nominal_dict_instance(
        &self,
        ty: Type,
        target: PythonTarget,
    ) -> Option<Type> {
        let node = self.node(self.expanded_alias_instance(ty))?;
        let Node::Container(Container::Dict, arguments) = node.as_ref() else {
            return None;
        };
        let instance = self.runtime_builtin_class_instance("dict", target)?;
        let id = self.class_id(instance)?;
        if arguments.len() != self.class(id).generic.len() {
            return None;
        }
        // AppliedClass cannot re-enter this compact-only fallback.
        Some(self.applied_class(id, arguments.clone(), false))
    }

    pub(super) fn dict_method(&self, ty: Type, name: &str) -> Option<Arc<Signature>> {
        let ty = self.expanded_alias_instance(ty);
        if name == "__getitem__" {
            let (key, value) = self.mapping_types(ty)?;
            let mut signature = Signature::empty(name.into(), value);
            signature.constructor = false;
            signature.owner = Some("dict".into());
            signature.bound_receiver = Some("self".into());
            signature.parameters = Arc::from([Parameter {
                name: String::new(),
                ty: Some(key),
                kind: Kind::PositionalOnly,
                required: true,
            }]);
            return Some(Arc::new(signature));
        }
        let node = self.node(ty)?;
        let Node::Container(Container::Dict, arguments) = node.as_ref() else {
            return None;
        };
        let key = arguments[0];
        let value = arguments[1];
        let view = match name {
            "keys" => Some(Container::DictKeys),
            "values" => Some(Container::DictValues),
            "items" => Some(Container::DictItems),
            _ => None,
        };
        if let Some(kind) = view {
            let mut signature = Signature::empty(
                name.into(),
                self.intern(Node::Container(kind, arguments.clone())),
            );
            signature.constructor = false;
            signature.owner = Some("dict".into());
            signature.bound_receiver = Some("self".into());
            return Some(Arc::new(signature));
        }
        if name != "get" {
            return None;
        }
        let variable = self.variable("_T".into(), Type::Object, Vec::new(), None);
        let mut root = Signature::empty(name.into(), Type::Any);
        root.constructor = false;
        root.owner = Some("dict".into());
        root.bound_receiver = Some("self".into());
        root.overloads = [
            (Type::None, self.union(vec![value, Type::None]), false),
            (value, value, true),
            (variable, self.union(vec![value, variable]), true),
        ]
        .into_iter()
        .map(|(default, returned, required)| {
            let mut variant = root.clone();
            variant.returns = Some(returned);
            variant.parameters = vec![
                Parameter {
                    name: "key".into(),
                    ty: Some(key),
                    kind: Kind::PositionalOnly,
                    required: true,
                },
                Parameter {
                    name: "default".into(),
                    ty: Some(default),
                    kind: Kind::PositionalOnly,
                    required,
                },
            ]
            .into();
            if default == variable {
                variant.generic = vec![variable].into();
            }
            Arc::new(variant)
        })
        .collect();
        Some(Arc::new(root))
    }
}
