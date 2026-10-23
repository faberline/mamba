//! Union attributes retain declared member results and known-error recovery.
use super::*;

impl Scopes {
    pub(in crate::infrastructure::mypy) fn set_super_receiver(
        &mut self,
        receiver: Option<(u32, Type)>,
    ) {
        self.stack.last_mut().expect("entered scope").super_receiver = receiver;
    }

    pub(in crate::infrastructure::mypy) fn super_method(
        &self,
        attribute: &mamba_language::application::quality_syntax::python_ast::ExprAttribute,
        environment: &BTreeMap<String, Type>,
    ) -> Option<Arc<Signature>> {
        self.super_method_binding(attribute, environment)
            .map(|(signature, _)| signature)
    }

    pub(in crate::infrastructure::mypy) fn super_method_binding(
        &self,
        attribute: &mamba_language::application::quality_syntax::python_ast::ExprAttribute,
        environment: &BTreeMap<String, Type>,
    ) -> Option<(Arc<Signature>, Option<String>)> {
        if !attribute.ctx.is_load() {
            return None;
        }
        let Expr::Call(call) = attribute.value.as_ref() else {
            return None;
        };
        if !call.arguments.args.is_empty() || !call.arguments.keywords.is_empty() {
            return None;
        }
        let (anchor, receiver) = self.current().super_receiver?;
        let target = self.context.module_target(&self.namespace);
        let factory = self
            .context
            .infer_with_calls(&call.func, environment, &|value| {
                self.expression_type(value, environment)
            })?;
        if !self.context.is_class_value(factory) {
            return None;
        }
        let factory = self.context.class_id(factory)?;
        let builtin = self
            .context
            .classes
            .borrow()
            .keys
            .get(&("builtins.super".into(), 0))
            .copied();
        if Some(factory) != builtin {
            let class = self.context.class(factory);
            let (project, _) = self.namespace.split_once(':')?;
            if self.context.embedded_source_class_name(factory).as_deref() != Some("builtins.super")
                || class.namespace.split_once(':')?.0 != project
                || self.context.module_target(&class.namespace) != target
            {
                return None;
            }
        }
        let order = self
            .context
            .class_mro(self.context.class_id(receiver)?)
            .ok()?;
        let start = order.iter().position(|&id| id == anchor)? + 1;
        for &owner in &order[start..] {
            let class = self.context.class(owner);
            if class.erased_members.contains(attribute.attr.as_str())
                || class.fields.contains_key(attribute.attr.as_str())
            {
                return None;
            }
            if class.methods.contains_key(attribute.attr.as_str()) {
                return self.context.class_method_binding_at(
                    receiver,
                    receiver,
                    owner,
                    attribute.attr.as_str(),
                    target,
                );
            }
        }
        None
    }

    pub(super) fn attribute_expression_type(
        &self,
        expression: &Expr,
        attribute: &mamba_language::application::quality_syntax::python_ast::ExprAttribute,
        environment: &BTreeMap<String, Type>,
    ) -> Option<Type> {
        if let Some(signature) = self.native_dataclass_function(expression) {
            return Some(self.context.callable_type(signature));
        }
        if let Some(ty) =
            Self::attribute_key(expression).and_then(|key| environment.get(&key).copied())
        {
            return Some(ty);
        }
        if let Some(signature) = self.super_method(attribute, environment) {
            return Some(self.context.callable_type(signature));
        }
        match self.module_member(expression) {
            Some(export) => export.ty,
            None => {
                let ty =
                    self.context
                        .infer_with_calls(&attribute.value, environment, &|expression| {
                            self.expression_type(expression, environment)
                        });
                if let Some(receiver) = ty {
                    if matches!(
                        self.context.node(receiver).as_deref(),
                        Some(types::Node::Union(_))
                    ) {
                        return self.attribute_member_type(receiver, attribute.attr.as_str());
                    }
                }
                if ty.is_some_and(|ty| ty.is_any()) {
                    return Some(Type::Any);
                }
                if let Some(method) =
                    ty.and_then(|ty| self.class_member_signature(ty, attribute.attr.as_str()))
                {
                    return Some(self.context.callable_type(method));
                }
                if let Some(ty) = ty.filter(|&ty| {
                    matches!(
                        self.context.node(ty).as_deref(),
                        Some(types::Node::Callable(_))
                    )
                }) {
                    return Some(
                        self.context
                            .callable_attribute(ty, attribute.attr.as_str())
                            .unwrap_or(Type::Any),
                    );
                }
                ty.and_then(|ty| self.context.class_field(ty, attribute.attr.as_str()))
                    .and_then(|field| field.ty)
                    .or_else(|| {
                        (self.module_expression(&attribute.value).is_some()
                            || ty.is_some_and(|ty| {
                                self.context.class_id(ty).is_some()
                                    || self.context.builtin_scalar(ty).is_some()
                            }))
                        .then_some(Type::Any)
                    })
            }
        }
    }

    pub(super) fn attribute_member_type(&self, receiver: Type, name: &str) -> Option<Type> {
        if receiver.is_any() {
            return Some(Type::Any);
        }
        if let Some(node) = self.context.node(receiver) {
            if let types::Node::Union(items) = node.as_ref() {
                let results = items
                    .iter()
                    .map(|&item| self.attribute_member_type(item, name))
                    .collect::<Option<Vec<_>>>()?;
                return Some(self.context.union(results));
            }
        }
        // None delegates supported members to object, as in the original checker.
        let lookup = if receiver == Type::None {
            Type::Object
        } else {
            receiver
        };
        if let Some(method) = self.class_member_signature(lookup, name) {
            return Some(self.context.callable_type(method));
        }
        if matches!(
            self.context.node(lookup).as_deref(),
            Some(types::Node::Callable(_))
        ) {
            return Some(
                self.context
                    .callable_attribute(lookup, name)
                    .unwrap_or(Type::Any),
            );
        }
        if let Some(field) = self.context.class_field(lookup, name) {
            // A declared but unresolved type is not a missing member.
            return field.ty;
        }
        if self.context.class_method(lookup, "__getattr__").is_some() {
            // An unresolved dynamic return is not proof of a missing member.
            return None;
        }
        let target = self.context.module_target(&self.namespace);
        if receiver == Type::None {
            // Keep special methods on their existing path until modeled.
            return (!(name.starts_with("__") && name.ends_with("__"))
                && self
                    .context
                    .missing_builtin_attribute(Type::Object, name, target)
                    .is_some())
            .then_some(Type::Any);
        }
        // A nominal ID can name an unloaded declaration, so it proves no absence.
        // Only the existing scalar symbol lookup confirms a missing member here.
        self.context
            .missing_builtin_attribute(receiver, name, target)
            .is_some()
            .then_some(Type::Any)
    }
}
