//! Class values, instances, and inherited members use nominal identity.
use super::super::{
    classes::{AnyBaseOrigin, Field, MethodKind},
    signatures::Signature,
};
use super::{Container, Context, Node, Type};
use mamba_language::application::quality_syntax::PythonTarget;
use std::sync::Arc;
mod embedded_methods;
mod metaclass;

impl Context {
    pub(super) fn instance_fallback_to_any(&self, ty: Type) -> bool {
        if !matches!(
            self.node(ty).as_deref(),
            Some(Node::Instance(_) | Node::AppliedClass { value: false, .. })
        ) {
            return false;
        }
        let Some(id) = self.class_id(ty) else {
            return false;
        };
        self.class_mro(id).is_ok_and(|lineage| {
            lineage.into_iter().any(|owner| {
                self.class(owner)
                    .any_base_origins
                    .iter()
                    .any(|origin| match origin {
                        AnyBaseOrigin::ResolvedAny { range } => !range.is_empty(),
                        AnyBaseOrigin::MissingModuleMember {
                            range,
                            module,
                            member,
                        } => !range.is_empty() && !module.is_empty() && !member.is_empty(),
                    })
            })
        })
    }

    pub(in crate::infrastructure::mypy) fn class_id(&self, ty: Type) -> Option<u32> {
        match self.node(ty)?.as_ref() {
            Node::Class(id) | Node::Instance(id) | Node::AppliedClass { id, .. } => Some(*id),
            Node::Container(Container::TypeObject, items) => self.type_object_class_id(items[0]),
            Node::Variable(variable) if self.is_source_self_variable(ty) => {
                self.class_id(variable.bound)
            }
            _ => None,
        }
    }
    fn type_object_class_id(&self, mut item: Type) -> Option<u32> {
        let mut active = std::collections::BTreeSet::new();
        while active.insert(item) {
            if let Some(Node::Variable(variable)) = self.node(item).as_deref() {
                item = variable.bound;
                continue;
            }
            return self.class_id(item).or_else(|| {
                self.builtin_scalar(item)
                    .map(|scalar| self.builtin_class(scalar))
            });
        }
        None
    }
    pub(in crate::infrastructure::mypy) fn instance_type(&self, ty: Type) -> Option<Type> {
        if let Some(Node::Container(Container::TypeObject, items)) = self.node(ty).as_deref() {
            return Some(items[0]);
        }
        if self.is_source_self_variable(ty) {
            return Some(ty);
        }
        if let Some(scalar) = self.builtin_scalar(ty) {
            return Some(scalar);
        }
        Some(self.applied_class(self.class_id(ty)?, self.class_arguments(ty), false))
    }
    pub(in crate::infrastructure::mypy) fn is_class_value(&self, ty: Type) -> bool {
        self.node(ty).is_some_and(|node| {
            matches!(
                node.as_ref(),
                Node::Class(_)
                    | Node::AppliedClass { value: true, .. }
                    | Node::Container(Container::TypeObject, _)
            )
        })
    }
    pub(in crate::infrastructure::mypy) fn class_lineage(&self, id: u32) -> Vec<u32> {
        self.class_mro(id).unwrap_or_else(|_| vec![id])
    }
    pub(in crate::infrastructure::mypy) fn scalar_base_accepts(
        &self,
        expected: Type,
        id: u32,
    ) -> bool {
        self.class_lineage(id).into_iter().any(|id| {
            self.class(id)
                .scalar_bases
                .iter()
                .any(|&base| self.accepts(expected, base))
        })
    }
    pub(in crate::infrastructure::mypy) fn nominal_accepts(
        &self,
        expected: u32,
        actual: u32,
    ) -> bool {
        if let Some(definition) = &self.class(expected).typed_dict {
            let pair = (expected, actual);
            if !self.classes.borrow_mut().typed_dict_checks.insert(pair) {
                return true;
            }
            let compatible = self
                .class(actual)
                .typed_dict
                .as_ref()
                .is_some_and(|actual| self.typed_dict_accepts(definition, actual));
            self.classes.borrow_mut().typed_dict_checks.remove(&pair);
            return compatible;
        }
        self.class_lineage(actual).contains(&expected)
    }
    pub(in crate::infrastructure::mypy) fn class_field(
        &self,
        ty: Type,
        name: &str,
    ) -> Option<Field> {
        self.class_field_access(ty, name, None)
    }

    pub(in crate::infrastructure::mypy) fn class_field_for_store(
        &self,
        ty: Type,
        name: &str,
        target: PythonTarget,
    ) -> Option<Field> {
        self.class_field_access(ty, name, Some(target))
    }

    fn class_field_access(
        &self,
        ty: Type,
        name: &str,
        store_target: Option<PythonTarget>,
    ) -> Option<Field> {
        if let Some(meta) = self.metaclass_member_receiver(ty, name, self.class_object_target(ty)) {
            return self.metaclass_field(ty, meta, name);
        }
        let mut field = self.raw_class_field_access(ty, name, store_target)?;
        if let Some(descriptor) = self.descriptor_member(ty, name) {
            if !self.descriptor_implicit(ty, name)
                && (self.class_member_owner(descriptor, "__get__").is_some()
                    || self.class_member_owner(descriptor, "__set__").is_some())
            {
                field.class_access = true;
            }
            if let Some(result) = self.descriptor_read_type(descriptor, ty) {
                field.ty = Some(result);
            }
        }
        Some(field)
    }

    pub(in crate::infrastructure::mypy) fn raw_class_field(
        &self,
        ty: Type,
        name: &str,
    ) -> Option<Field> {
        self.raw_class_field_access(ty, name, None)
    }

    fn raw_class_field_access(
        &self,
        ty: Type,
        name: &str,
        store_target: Option<PythonTarget>,
    ) -> Option<Field> {
        if let Some(meta) = self.metaclass_member_receiver(ty, name, self.class_object_target(ty)) {
            let mut field = self.raw_class_field(meta, name)?;
            field.class_access = true;
            return Some(field);
        }
        for id in self.class_lineage(self.class_id(ty)?) {
            if self.class(id).erased_members.contains(name) {
                return Some(Field {
                    ty: Some(Type::Any),
                    needs_annotation: false,
                    class_access: true,
                    readonly: false,
                    final_attribute: false,
                    class_var: false,
                    abstract_field: false,
                });
            }
            if let Some(field) = self.class(id).fields.get(name) {
                let mut field = field.clone();
                field.ty = field
                    .ty
                    .map(|member| self.substitute(member, &self.class_solutions(ty, id)));
                return Some(field);
            }
            if let Some(method) = self.class(id).methods.get(name) {
                if method.kind == MethodKind::Property {
                    return Some(Field {
                        needs_annotation: false,
                        ty: method
                            .signature
                            .returns
                            .map(|member| self.substitute(member, &self.class_solutions(ty, id))),
                        class_access: false,
                        readonly: method.setter.is_none()
                            && !self.class(id).cached_properties.contains(name),
                        final_attribute: false,
                        class_var: false,
                        abstract_field: method.abstractness
                            != super::super::classes::Abstractness::Concrete,
                    });
                }
            }
        }
        if !self.is_class_value(ty)
            && name != "__getattr__"
            && self.class_method(ty, name).is_none()
        {
            if let Some(returns) = self
                .class_method(ty, "__getattr__")
                .and_then(|signature| signature.returns)
            {
                // A dynamic read is not a write declaration. Unknown member sets keep
                // the old result; only a closed native MRO proves this field absent.
                if store_target.is_none_or(|target| !self.closed_native_store_members(ty, target)) {
                    return Some(Field {
                        needs_annotation: false,
                        ty: Some(returns),
                        class_access: false,
                        readonly: false,
                        final_attribute: false,
                        class_var: false,
                        abstract_field: false,
                    });
                }
            }
        }
        if self.instance_fallback_to_any(ty) && self.class_member_owner(ty, name).is_none() {
            return Some(Field {
                ty: Some(Type::Any),
                needs_annotation: false,
                class_access: true,
                readonly: false,
                final_attribute: false,
                class_var: false,
                abstract_field: false,
            });
        }
        None
    }
    fn closed_native_store_members(&self, ty: Type, target: PythonTarget) -> bool {
        if !matches!(
            self.node(ty).as_deref(),
            Some(Node::Instance(_) | Node::AppliedClass { value: false, .. })
        ) {
            return false;
        }
        let Some(id) = self.class_id(ty) else {
            return false;
        };
        let Some(lineage) = self.class_mro(id).ok() else {
            return false;
        };
        // Native bases are shared across target loads. Do not use an earlier
        // target's retained bases to prove absence for another target.
        if !self.native_members_loaded(target)
            || self
                .classes
                .borrow()
                .builtin_value_libraries
                .iter()
                .filter(|&&loaded| loaded)
                .count()
                != 1
        {
            return false;
        }
        let object = self.builtin_class(Type::Object);
        for &owner in &lineage {
            let class = self.class(owner);
            let has_slots = self
                .classes
                .borrow()
                .embedded_class_slots
                .contains_key(&owner);
            if class.plugin
                || class.protocol
                || class.dataclass.is_some()
                || class.pydantic.is_some()
                || has_slots
                || !class.any_base_origins.is_empty()
                || class.erased_members.contains("__setattr__")
                || class
                    .scalar_bases
                    .iter()
                    .any(|&base| owner != object || base != Type::Object)
                || class.base_types.len() != class.bases.len()
                || !class
                    .base_types
                    .iter()
                    .zip(&class.bases)
                    .all(|(&base, &id)| self.class_id(base) == Some(id))
                || !class.bases.iter().all(|base| lineage.contains(base))
                || self.class_mro(owner).is_err()
                || (owner != object && !self.loaded_native_class(owner, &class.namespace, target))
            {
                return false;
            }
        }
        // A real custom setter, even with an unknown signature, retains acceptance.
        self.class_member_owner(ty, "__setattr__")
            .is_none_or(|owner| owner == object)
    }

    fn native_members_loaded(&self, target: PythonTarget) -> bool {
        let slot = match target {
            PythonTarget::Py312 => 0,
            PythonTarget::Py313 => 1,
            PythonTarget::Py314 => 2,
        };
        self.classes.borrow().builtin_value_libraries[slot]
    }

    fn loaded_native_class(&self, id: u32, name: &str, target: PythonTarget) -> bool {
        // The completed library flag makes this query a lookup, not a loader.
        self.native_members_loaded(target)
            && self
                .builtin_value_class_instance(name, target)
                .and_then(|ty| self.class_id(ty))
                == Some(id)
    }

    pub(in crate::infrastructure::mypy) fn is_module_type_instance(
        &self,
        ty: Type,
        target: PythonTarget,
    ) -> bool {
        matches!(
            self.node(ty).as_deref(),
            Some(Node::Instance(_) | Node::AppliedClass { value: false, .. })
        ) && self.class_id(ty).is_some_and(|id| {
            self.embedded_source_class_name(id).as_deref() == Some("types.ModuleType")
                || self.loaded_native_class(id, "types.ModuleType", target)
        })
    }

    pub(in crate::infrastructure::mypy) fn class_method(
        &self,
        ty: Type,
        name: &str,
    ) -> Option<Arc<Signature>> {
        self.class_method_for(ty, name, PythonTarget::Py312)
    }
    pub(in crate::infrastructure::mypy) fn class_method_for(
        &self,
        ty: Type,
        name: &str,
        target: PythonTarget,
    ) -> Option<Arc<Signature>> {
        self.class_method_binding_for(ty, name, target)
            .map(|binding| binding.signature)
    }

    // A protocol call needs the signature and error from the same binding.
    pub(in crate::infrastructure::mypy) fn class_method_call_binding_for(
        &self,
        ty: Type,
        name: &str,
        target: PythonTarget,
    ) -> Option<(Arc<Signature>, Option<String>)> {
        self.class_method_binding_for(ty, name, target)
            .map(|binding| (binding.signature, binding.error))
    }

    pub(in crate::infrastructure::mypy) fn method_receiver_error(
        &self,
        ty: Type,
        name: &str,
        target: PythonTarget,
    ) -> Option<String> {
        self.class_method_binding_for(ty, name, target)?.error
    }
    fn class_method_binding_for(
        &self,
        ty: Type,
        name: &str,
        target: PythonTarget,
    ) -> Option<super::method_bindings::BoundMethod> {
        let class_object = ty;
        let metaclass = self.metaclass_member_receiver(ty, name, target);
        let ty = metaclass.unwrap_or(ty);
        if let Some(member) = self.builtin_constructor_member(ty, name, target) {
            return Some(super::method_bindings::BoundMethod::plain(member));
        }
        let ty = self.template_member_receiver(ty, name);
        let scalar = match self.node(ty).as_deref() {
            Some(Node::Literal(value) | Node::LiteralValue(value)) => value.base(),
            _ => ty,
        };
        let scalar = if scalar == Type::Bool {
            Type::Int
        } else {
            scalar
        };
        let comparison = self
            .classes
            .borrow()
            .embedded_comparisons
            .get(&(scalar, name.into()))
            .cloned();
        if comparison.is_some() {
            return comparison.map(super::method_bindings::BoundMethod::plain);
        }
        if let Some(signature) = self.embedded_method(ty, name, target) {
            return Some(super::method_bindings::BoundMethod::plain(signature));
        }
        let id = self.class_id(ty).or_else(|| {
            self.builtin_scalar(ty)
                .map(|scalar| self.builtin_class(scalar))
        })?;
        for id in self.class_lineage(id) {
            let class = self.class(id);
            if class.erased_members.contains(name) {
                return None;
            }
            if class.methods.contains_key(name) {
                let self_receiver = metaclass.map_or(ty, |_| class_object);
                return self.bind_class_method_at(ty, self_receiver, id, name, target);
            }
        }
        if name == "model_construct" {
            // Synthetic model methods belong to the original class, not its metaclass.
            if let Some(id) = self.class_id(class_object) {
                if let Some(model) = &self.class(id).pydantic {
                    return Some(super::method_bindings::BoundMethod::plain(Arc::new(
                        self.pydantic_construct_method(class_object, model),
                    )));
                }
            }
        }
        None
    }
    pub(in crate::infrastructure::mypy) fn class_constructor(
        &self,
        ty: Type,
    ) -> Option<Arc<Signature>> {
        self.class_constructor_for(ty, PythonTarget::Py312)
    }
    pub(in crate::infrastructure::mypy) fn class_constructor_for(
        &self,
        ty: Type,
        target: PythonTarget,
    ) -> Option<Arc<Signature>> {
        if let Some(signature) = self.builtin_value_constructor(ty, target) {
            return Some(signature);
        }
        if matches!(
            self.node(ty).as_deref(),
            Some(Node::Container(Container::TypeObject, _))
        ) {
            let mut constructors = self.type_object_constructors_for(ty, target)?;
            return (constructors.len() == 1).then(|| constructors.remove(0));
        }
        if !self.is_class_value(ty) {
            return self.class_method(ty, "__call__");
        }
        let class = self.class(self.class_id(ty)?);
        if let Some(signature) = self.collection_class_constructor(ty, target) {
            return Some(signature);
        }
        if class.plugin && class.pydantic.is_none() && !self.has_declared_initializer(ty) {
            return None;
        }
        let instance = self.instance_type(ty)?;
        let mut signature = class
            .pydantic
            .as_ref()
            .filter(|_| !self.pydantic_declared_initializer(ty))
            .map_or_else(
                || (*self.constructor_signature(ty, instance, target)).clone(),
                |model| self.pydantic_constructor(ty, instance, model),
            );
        if !self
            .node(ty)
            .is_some_and(|node| matches!(node.as_ref(), Node::AppliedClass { .. }))
        {
            prepend_constructor_binders(&mut signature, &class.generic);
        }
        Some(Arc::new(signature))
    }
    pub(in crate::infrastructure::mypy) fn class_method_binding_at(
        &self,
        ty: Type,
        self_receiver: Type,
        id: u32,
        name: &str,
        target: PythonTarget,
    ) -> Option<(Arc<Signature>, Option<String>)> {
        self.bind_class_method_at(ty, self_receiver, id, name, target)
            .map(|binding| (binding.signature, binding.error))
    }

    pub(super) fn supported_method_receiver(&self, template: Type, allow_callable: bool) -> bool {
        match self.node(template).as_deref() {
            Some(Node::Variable(variable)) => !variable.parameter_spec && !variable.type_var_tuple,
            Some(Node::Container(Container::TypeObject, items)) => {
                self.supported_method_receiver(items[0], true)
            }
            Some(Node::Callable(_)) => allow_callable,
            Some(Node::AppliedClass {
                id,
                arguments,
                value: false,
            }) => arguments != &self.class(*id).generic,
            _ => false,
        }
    }
}

fn prepend_constructor_binders(signature: &mut Signature, binders: &[Type]) {
    if binders.is_empty() {
        return;
    }
    signature.generic = binders
        .iter()
        .copied()
        .chain(signature.generic.iter().copied())
        .collect::<Vec<_>>()
        .into();
    for variant in Arc::make_mut(&mut signature.overloads) {
        prepend_constructor_binders(Arc::make_mut(variant), binders);
    }
}
