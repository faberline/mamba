//! Runtime class unions and buffer members retain locked builtin identities.
use super::*;
use crate::infrastructure::mypy::signatures::Signature;

impl Context {
    pub(in crate::infrastructure::mypy) fn builtin_buffer_member(
        &self,
        ty: Type,
        name: &str,
        target: PythonTarget,
    ) -> Option<Arc<Signature>> {
        if name != "__buffer__" {
            return None;
        }
        let scalar = match self.node(ty).as_deref() {
            Some(Node::Literal(value) | Node::LiteralValue(value)) => value.base(),
            _ => ty,
        };
        if scalar != Type::Bytes {
            return None;
        }
        let value = data(target)["classes"]["builtins.bytes"]["methods"].get(name)?;
        self.load_builtin_value_classes(target);
        let key = (slot(target), ty, name.to_string());
        if let Some(signature) = self.classes.borrow().builtin_value_members.get(&key) {
            return Some(signature.clone());
        }
        let mut reader = self.builtin_value_reader(target);
        let signature = Arc::new(
            signatures::read(&mut reader, &value["type"], "builtins", Some("bytes")).bound(1),
        );
        self.classes
            .borrow_mut()
            .builtin_value_members
            .insert(key, signature.clone());
        Some(signature)
    }

    // Compact list pop and append use locked declarations with the actual item type.
    pub(in crate::infrastructure::mypy) fn builtin_list_item_member(
        &self,
        ty: Type,
        name: &str,
        target: PythonTarget,
    ) -> Option<Arc<Signature>> {
        if !matches!(name, "pop" | "append") {
            return None;
        }
        let node = self.node(ty)?;
        let Node::Container(Container::List, arguments) = node.as_ref() else {
            return None;
        };
        let [item] = arguments.as_slice() else {
            return None;
        };
        let class = &data(target)["classes"]["builtins.list"];
        let value = class["methods"].get(name)?;
        if value["class_method"].as_bool()?
            || value["static_method"].as_bool()?
            || value["property"].as_bool()?
        {
            return None;
        }
        let [parameter] = class["generic"].as_array()?.as_slice() else {
            return None;
        };
        self.load_builtin_value_classes(target);
        let key = (slot(target), ty, name.to_string());
        if let Some(signature) = self.classes.borrow().builtin_value_members.get(&key) {
            return Some(signature.clone());
        }
        let mut reader = self.builtin_value_reader(target);
        let variable = reader.ty(parameter);
        if !matches!(self.node(variable).as_deref(), Some(Node::Variable(_))) {
            return None;
        }
        let signature = signatures::read(&mut reader, &value["type"], "builtins", Some("list"));
        let signature = Arc::new(
            self.substitute_callable(&signature, &BTreeMap::from([(variable, *item)]))
                .bound(1),
        );
        self.classes
            .borrow_mut()
            .builtin_value_members
            .insert(key, signature.clone());
        Some(signature)
    }

    pub(in crate::infrastructure::mypy) fn runtime_class_union(
        &self,
        left: Type,
        right: Type,
    ) -> Option<Type> {
        let supported = |ty| {
            self.is_class_value(ty)
            || self.class_id(ty).is_some_and(|id| self.class(id).namespace == "types.UnionType")
            || self.node(ty).is_some_and(|node| matches!(node.as_ref(),Node::Callable(signature) if signature.constructor))
        };
        (supported(left) && supported(right)).then(|| {
            let id = self.declare_class("types.UnionType", "UnionType", 0);
            self.class(id).instance
        })
    }
}
