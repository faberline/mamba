//! Raw function declarations and typed decorator results retain separate identities.
use super::*;
use mamba_language::application::quality_syntax::python_ast::{
    Arguments, ExprCall, ExprContext, ExprName, StmtClassDef,
};

mod methods;

pub(in crate::infrastructure::mypy) struct DecoratedFunction {
    pub calls: Vec<(ExprCall, Type, Arc<Signature>)>,
    pub result: Option<Type>,
}

impl Scopes {
    pub(in crate::infrastructure::mypy) const DECORATOR_ARGUMENT: &'static str =
        "\0decorator-argument";

    pub(super) fn raw_function_signature(&self, name: &str) -> Option<Arc<Signature>> {
        let scope = self.current();
        let binding = scope.bindings.get(name)?;
        scope.raw_functions.get(&binding.first_definition).cloned()
    }

    pub(super) fn replace_raw_function_signature(&mut self, name: &str, signature: Arc<Signature>) {
        let Some(definition) = self
            .current()
            .bindings
            .get(name)
            .map(|binding| binding.first_definition)
        else {
            return;
        };
        if !self.current().raw_functions.contains_key(&definition) {
            return;
        }
        let ty = self.context.callable_type(signature.clone());
        let scope = self.stack.last_mut().unwrap();
        scope.raw_functions.insert(definition, signature.clone());
        let binding = scope.bindings.get_mut(name).unwrap();
        binding.ty = Some(ty);
        binding.annotation_form = None;
        binding.annotation_module = None;
        binding.callable = Some(signature);
    }

    pub(super) fn install_function_declaration(
        &mut self,
        function: &StmtFunctionDef,
        signature: Arc<Signature>,
    ) -> Type {
        self.stack
            .last_mut()
            .unwrap()
            .raw_functions
            .insert(function.name.start(), signature.clone());
        let ty = self.context.callable_type(signature.clone());
        self.stack.last_mut().unwrap().bindings.insert(
            function.name.to_string(),
            Binding {
                ty: Some(ty),
                first_definition: function.name.start(),
                infer_from_assignment: false,
                callable: Some(signature),
                module: None,
                explicit: true,
                module_hidden: Some(false),
                annotation_form: None,
                annotation_module: None,
            },
        );
        ty
    }

    pub(super) fn refresh_function_decorators(&mut self, body: &[Stmt]) -> bool {
        let mut changed = false;
        for statement in body {
            if let Stmt::FunctionDef(function) = statement {
                changed |= self.apply_function_decorators(function);
            }
        }
        changed
    }

    pub(super) fn apply_function_decorators(&mut self, function: &StmtFunctionDef) -> bool {
        let applied = self.function_decorator_application(function);
        if applied.calls.is_empty() {
            return false;
        }
        let Some(mut result) = applied.result else {
            return false;
        };
        let Some(raw) = self.function_declaration_signature(function) else {
            return false;
        };
        if let Some(node) = self.context.node(result) {
            if let types::Node::Callable(signature) = node.as_ref() {
                result = self.context.callable_type(Arc::new(Signature {
                    name: raw.name.clone(),
                    owner: raw.owner.clone(),
                    plain_source_implicit_name: false,
                    ..signature.as_ref().clone()
                }));
            }
        }
        let callable = self
            .context
            .callable_signature_for(result, self.context.module_target(&self.namespace));
        let binding = self
            .stack
            .last_mut()
            .unwrap()
            .bindings
            .get_mut(function.name.as_str())
            .unwrap();
        let changed = binding.ty != Some(result) || binding.callable != callable;
        binding.ty = Some(result);
        binding.annotation_form = None;
        binding.annotation_module = None;
        binding.callable = callable;
        changed
    }

    pub(in crate::infrastructure::mypy) fn function_decorator_application(
        &self,
        function: &StmtFunctionDef,
    ) -> DecoratedFunction {
        let applied = DecoratedFunction {
            calls: Vec::new(),
            result: None,
        };
        // Class descriptors and special declaration markers retain their existing paths.
        if self.current().kind == Kind::Class
            || function.decorator_list.is_empty()
            || super::super::signatures::no_type_check(function)
            || self.is_overload(function)
        {
            return applied;
        }
        let Some(raw) = self.function_declaration_signature(function) else {
            return applied;
        };
        self.typed_decorator_application(function, raw, self.environment())
    }

    fn typed_decorator_application(
        &self,
        function: &StmtFunctionDef,
        raw: Arc<Signature>,
        mut environment: BTreeMap<String, Type>,
    ) -> DecoratedFunction {
        let mut applied = DecoratedFunction {
            calls: Vec::new(),
            result: None,
        };
        let mut argument = self.context.callable_type(raw);
        for decorator in function.decorator_list.iter().rev() {
            let expression = &decorator.expression;
            let Some(value) = self
                .context
                .infer_with_calls(expression, &environment, &|value| {
                    self.expression_type(value, &environment)
                })
            else {
                return applied;
            };
            if value.is_any() {
                return applied;
            }
            let Some(signature) = self.callable_expression_in(expression, &environment) else {
                return applied;
            };
            if matches!(
                signature.native_symbol,
                Some("typing.final" | "typing.override" | "typing.overload" | "abc.abstractmethod")
            ) {
                continue;
            }
            if signature.implicit
                || signature.ignored
                || signature.constructor
                || signature.native_symbol.is_some()
            {
                return applied;
            }
            let call = ExprCall {
                node_index: Default::default(),
                range_start: expression.start(),
                func: Box::new(expression.clone()),
                arguments: Arguments {
                    range: expression.range(),
                    node_index: Default::default(),
                    args: vec![Expr::Name(ExprName {
                        node_index: Default::default(),
                        range: expression.range(),
                        id: Self::DECORATOR_ARGUMENT.into(),
                        ctx: ExprContext::Load,
                    })]
                    .into(),
                    keywords: Default::default(),
                },
            };
            environment.insert(Self::DECORATOR_ARGUMENT.into(), argument);
            let result = if signature.overloads.is_empty() {
                let solution = self.call_solution(&call, &signature, &environment, None);
                if !solution.unresolved.is_empty()
                    || !solution.failed.is_empty()
                    || !solution.invalid.is_empty()
                {
                    None
                } else {
                    self.call_return_context(&call, &environment, None)
                }
            } else {
                let resolution = self.resolve_overload(&call, &signature, &environment, None);
                resolution.matched.then_some(resolution.returns)
            };
            applied.calls.push((call, argument, signature));
            let Some(result) = result else {
                return applied;
            };
            argument = result;
        }
        applied.result = Some(argument);
        applied
    }

    pub(in crate::infrastructure::mypy) fn invalid_computed_decorator(
        &self,
        call: &ExprCall,
    ) -> bool {
        let environment = self.environment();
        let Some(signature) = self.callable_expression_in(&call.func, &environment) else {
            return false;
        };
        if signature.native_symbol != Some("pydantic.fields.computed_field") {
            return false;
        }
        let resolution = self.resolve_overload(call, &signature, &environment, None);
        !resolution.matched && resolution.fallback.is_none()
    }

    pub(super) fn refresh_class_decorators(&self, id: u32, declaration: &StmtClassDef) {
        let mut class = (*self.context.class(id)).clone();
        for statement in &declaration.body {
            let Stmt::FunctionDef(function) = statement else {
                continue;
            };
            if let Some(method) = class.methods.get(function.name.as_str()).cloned() {
                if let Some(signature) =
                    self.decorated_instance_method(declaration, function, &method)
                {
                    class
                        .methods
                        .get_mut(function.name.as_str())
                        .unwrap()
                        .signature = signature;
                }
            }
            if function.decorator_list.iter().any(|decorator| {
                matches!(&decorator.expression, Expr::Call(call) if self.invalid_computed_decorator(call))
            }) {
                class.erased_members.insert(function.name.to_string());
            }
        }
        self.context.replace_class(id, class);
    }
}
