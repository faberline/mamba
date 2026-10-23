//! Yield expression results belong to the current function frame only.
use super::*;
use mamba_language::application::quality_syntax::python_ast::ExprGenerator;

impl Scopes {
    // The source callable Arc is already resolved. These APIs only inspect the
    // live source binding; they do not repeat callable/provider inference.
    pub(in crate::infrastructure::mypy) fn candidate_source_callable(
        &self,
        expression: &Expr,
        signature: &Arc<Signature>,
        environment: &BTreeMap<String, Type>,
    ) -> Option<(String, String, TextSize, Type, Arc<Signature>)> {
        let Expr::Name(name) = expression else {
            return None;
        };
        if !self.candidate_function_role(expression, signature, environment) {
            return None;
        }
        let binding = self.resolved_binding(name.id.as_str())?;
        if !Arc::ptr_eq(binding.callable.as_ref()?, signature) {
            return None;
        }
        let receipt = (
            self.namespace.clone(),
            name.id.to_string(),
            binding.first_definition,
            binding.ty?,
            signature.clone(),
        );
        self.candidate_source_callable_is_current(&receipt, environment)
            .then_some(receipt)
    }

    pub(in crate::infrastructure::mypy) fn candidate_source_callable_is_current(
        &self,
        receipt: &(String, String, TextSize, Type, Arc<Signature>),
        environment: &BTreeMap<String, Type>,
    ) -> bool {
        let (namespace, name, definition, ty, signature) = receipt;
        let Some(binding) = self.resolved_binding(name) else {
            return false;
        };
        namespace == &self.namespace
            && !self.current().redirects.contains_key(name)
            && !binding.infer_from_assignment
            && binding.module.is_none()
            && binding.annotation_form.is_none()
            && binding.annotation_module.is_none()
            && binding.first_definition == *definition
            && binding.ty == Some(*ty)
            && environment.get(name) == Some(ty)
            && binding
                .callable
                .as_ref()
                .is_some_and(|actual| Arc::ptr_eq(actual, signature))
            && self.stack.iter().any(|scope| {
                scope
                    .bindings
                    .get(name)
                    .is_some_and(|actual| std::ptr::eq(actual, binding))
                    && scope
                        .raw_functions
                        .get(definition)
                        .is_some_and(|raw| Arc::ptr_eq(raw, signature))
            })
    }

    pub(in crate::infrastructure::mypy) fn candidate_bound_method_role(
        &self,
        expression: &Expr,
        signature: &Signature,
        binding: &super::super::modules::BoundCallable,
        environment: &BTreeMap<String, Type>,
    ) -> bool {
        let Expr::Attribute(attribute) = expression else {
            return false;
        };
        let Expr::Name(receiver) = attribute.value.as_ref() else {
            return false;
        };
        receiver.ctx.is_load()
            && binding.expression == expression as *const Expr as usize
            && binding.owner == self.try_input_scope()
            && std::ptr::eq(binding.signature.as_ref(), signature)
            && binding.error.is_none()
            && environment.get(receiver.id.as_str()).copied() == Some(binding.receiver)
            && !binding.receiver.is_any()
            && binding.receiver != Type::Never
            && self.context.candidate_value_role(binding.receiver, 0)
            && !self.context.is_class_value(binding.receiver)
            && signature.name.as_deref() == Some(attribute.attr.as_str())
            && signature.owner.is_some()
            && signature.bound_receiver.is_some()
            && !signature.implicit
            && !signature.constructor
            && !signature.ignored
            && !signature.ellipsis
            && signature.narrowing.is_none()
    }

    fn candidate_builtin_constructor_role(
        &self,
        expression: &Expr,
        signature: &Signature,
        environment: &BTreeMap<String, Type>,
    ) -> bool {
        let Expr::Name(name) = expression else {
            return false;
        };
        name.ctx.is_load()
            && !self.has_binding(name.id.as_str())
            && !environment.contains_key(name.id.as_str())
            && !self.stack.iter().any(|scope| {
                scope.variables.keys().any(|key| {
                    key == name.id.as_str()
                        || key
                            .strip_prefix(name.id.as_str())
                            .is_some_and(|tail| tail.starts_with('.'))
                })
            })
            && signature.definition_module.as_deref() == Some("builtins")
            && signature.name.as_deref() == Some(name.id.as_str())
            && signature.constructor
            && !signature.implicit
            && signature.owner.is_none()
            && signature.bound_receiver.is_none()
            && !signature.ignored
            && !signature.ellipsis
            && signature.narrowing.is_none()
            && signature.returns.is_some()
    }

    // The receiver is a real Name resolved by the ordinary callable lookup.
    // This does not infer the Name or repeat any provider query.
    pub(in crate::infrastructure::mypy) fn candidate_function_role(
        &self,
        expression: &Expr,
        signature: &Signature,
        environment: &BTreeMap<String, Type>,
    ) -> bool {
        let Expr::Name(name) = expression else {
            return false;
        };
        if !name.ctx.is_load()
            || self.current().redirects.contains_key(name.id.as_str())
            || signature.constructor
            || signature.owner.is_some()
            || signature.bound_receiver.is_some()
            || signature.ignored
            || signature.ellipsis
            || signature.narrowing.is_some()
        {
            return false;
        }
        if let Some(binding) = self.resolved_binding(name.id.as_str()) {
            if binding.infer_from_assignment
                || binding.module.is_some()
                || binding.annotation_form.is_some()
                || binding.annotation_module.is_some()
                || binding.ty.is_none()
                || environment.get(name.id.as_str()).copied() != binding.ty
            {
                return false;
            }
            let owner = self.stack.iter().find(|scope| {
                scope
                    .bindings
                    .get(name.id.as_str())
                    .is_some_and(|actual| std::ptr::eq(actual, binding))
            });
            return binding.callable.as_ref().is_some_and(|callable| {
                callable.as_ref() == signature
                    && binding.ty.and_then(|ty| self.context.node(ty)).is_some_and(|node| {
                        // Interning retains an earlier equal Signature Arc.
                        // Source provenance comes from the live owner below,
                        // not the allocation kept by the type graph.
                        matches!(node.as_ref(), types::Node::Callable(actual) if actual.as_ref() == callable.as_ref())
                    })
                    && owner.is_some_and(|scope| {
                        scope
                            .raw_functions
                            .get(&binding.first_definition)
                            .is_some_and(|raw| Arc::ptr_eq(raw, callable))
                    })
            });
        }
        // In callable_expression_in this unbound, non-type Name can only come
        // from the builtin function producers, not a value/alias/module lookup.
        !environment.contains_key(name.id.as_str())
            && !self.stack.iter().any(|scope| {
                scope.variables.keys().any(|key| {
                    key == name.id.as_str()
                        || key
                            .strip_prefix(name.id.as_str())
                            .is_some_and(|tail| tail.starts_with('.'))
                })
            })
            && (!name.id.as_str().starts_with('_') || name.id.as_str().starts_with("__"))
            && signature.definition_module.as_deref() == Some("builtins")
            && signature.name.as_deref() == Some(name.id.as_str())
    }

    pub(in crate::infrastructure::mypy) fn candidate_function(
        &self,
        expression: &Expr,
        environment: &BTreeMap<String, Type>,
    ) -> Option<Arc<Signature>> {
        let Expr::Name(name) = expression else {
            return None;
        };
        if self.has_binding(name.id.as_str()) {
            let signature = self.resolved_binding(name.id.as_str())?.callable.clone()?;
            return self
                .candidate_function_role(expression, &signature, environment)
                .then_some(signature);
        }
        if environment.contains_key(name.id.as_str())
            || self
                .stack
                .iter()
                .any(|scope| scope.variables.contains_key(name.id.as_str()))
        {
            return None;
        }
        let target = self.context.module_target(&self.namespace);
        // Same provider order as builtin_value_expression. Each producer runs
        // at most once here; a selected unsupported provider is terminal.
        if let Some(signature) = self
            .context
            .collection_constructor(name.id.as_str(), target)
        {
            return self
                .candidate_builtin_constructor_role(expression, &signature, environment)
                .then_some(signature);
        }
        let signature = if let Some(signature) = self
            .context
            .standard_collection_function(name.id.as_str(), target)
        {
            signature
        } else if let Some(signature) = self.context.aggregation_function(name.id.as_str(), target)
        {
            signature
        } else {
            self.context
                .name_builtin_function(name.id.as_str(), target)?
        };
        self.candidate_function_role(expression, &signature, environment)
            .then_some(signature)
    }

    pub(in crate::infrastructure::mypy) fn enter_candidate_generator(
        &mut self,
        generators: &[mamba_language::application::quality_syntax::python_ast::Comprehension],
        initial: &BTreeMap<String, Type>,
    ) -> BTreeMap<String, Type> {
        let owners: BTreeMap<_, _> = initial
            .keys()
            .map(|key| (key.clone(), self.comprehension_binding_identity(key)))
            .collect();
        self.enter_comprehension(generators);
        let mut environment = self.environment();
        for (key, &ty) in initial {
            let name = key.split(['.', '[']).next().unwrap_or(key);
            if !self.current().bindings.contains_key(name)
                && owners.get(key).copied().flatten().is_some_and(|identity| {
                    self.comprehension_binding_identity(key) == Some(identity)
                })
            {
                environment.insert(key.clone(), ty);
            }
        }
        environment
    }

    pub(in crate::infrastructure::mypy) fn bind_candidate_generator_target(
        &mut self,
        target: &Expr,
        item: Type,
    ) -> Option<BTreeMap<String, Type>> {
        fn collect(
            context: &types::Context,
            target: &Expr,
            item: Type,
            bindings: &mut BTreeMap<String, Type>,
        ) -> Option<()> {
            match target {
                Expr::Name(name) if name.ctx.is_store() => {
                    if bindings.insert(name.id.to_string(), item).is_some() {
                        return None;
                    }
                }
                Expr::Tuple(tuple) if tuple.ctx.is_store() => {
                    let node = context.node(item)?;
                    let types::Node::Tuple(items) = node.as_ref() else {
                        return None;
                    };
                    if tuple.elts.len() != items.len() {
                        return None;
                    }
                    for (target, &item) in tuple.elts.iter().zip(items) {
                        collect(context, target, item, bindings)?;
                    }
                }
                _ => return None,
            }
            Some(())
        }
        let mut bindings = BTreeMap::new();
        collect(&self.context, target, item, &mut bindings)?;
        // Use the normal binding producer once. The finite shape above rules
        // out its unknown-unpack recovery; a mismatch remains unproved.
        let actual = self.bind_comprehension_target(target, item);
        (actual == bindings).then_some(actual)
    }

    pub(super) fn comprehension_generator_type(
        &self,
        generator: &ExprGenerator,
        environment: &BTreeMap<String, Type>,
    ) -> Option<Type> {
        let (scope, environment) = self.comprehension_scope_with_item(
            &generator.generators,
            environment,
            &|ty| self.context.iterable_item(ty),
            true,
        )?;
        let item = self
            .context
            .infer_with_calls(&generator.elt, &environment, &|value| {
                scope.expression_type(value, &environment)
            })?;
        let asynchronous = generator.generators.iter().any(|value| value.is_async);
        Some(self.context.intern(types::Node::Container(
            if asynchronous {
                types::Container::AsyncGenerator
            } else {
                types::Container::Generator
            },
            if asynchronous {
                vec![item, Type::None]
            } else {
                vec![item, Type::None, Type::None]
            },
        )))
    }

    pub(super) fn comprehension_target(
        &self,
        target: &Expr,
        item: Type,
        environment: &mut BTreeMap<String, Type>,
    ) {
        match target {
            Expr::Name(name) => {
                environment.insert(name.id.to_string(), item);
            }
            Expr::Tuple(tuple) => {
                let node = self.context.node(item);
                for (index, target) in tuple.elts.iter().enumerate() {
                    let ty = match node.as_deref() {
                        Some(types::Node::Tuple(items)) => {
                            items.get(index).copied().unwrap_or(Type::Any)
                        }
                        _ => self.context.iterable_item(item).unwrap_or(Type::Any),
                    };
                    self.comprehension_target(target, ty, environment);
                }
            }
            _ => {}
        }
    }

    pub(in crate::infrastructure::mypy) fn set_generator(
        &mut self,
        generator: Option<(Type, bool)>,
        body: &[Stmt],
    ) {
        self.stack.last_mut().unwrap().generator = generator;
        if generator.is_some() {
            self.infer_declarations(body);
        }
    }

    pub(in crate::infrastructure::mypy) fn generator(&self) -> Option<(Type, bool)> {
        self.current().generator
    }

    pub(super) fn generator_expression_type(
        &self,
        expression: &Expr,
        environment: &BTreeMap<String, Type>,
    ) -> Option<Type> {
        let (declared, _) = self.generator()?;
        match expression {
            Expr::Yield(_) => Some(self.context.generator_parts(declared).receiving),
            Expr::YieldFrom(delegated) => Some(
                self.context
                    .infer_with_calls(&delegated.value, environment, &|value| {
                        self.expression_type(value, environment)
                    })
                    .and_then(|actual| self.context.delegated_generator(actual))
                    .map_or(Type::Any, |parts| parts.returning),
            ),
            _ => None,
        }
    }
}
