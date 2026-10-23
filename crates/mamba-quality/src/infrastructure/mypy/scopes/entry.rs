//! Scope entry retains lexical binders and separates builtin annotation names.
use super::*;
use mamba_language::application::quality_syntax::python_ast::{
    visitor::{walk_except_handler, walk_expr, walk_stmt, Visitor},
    ExceptHandler, ExprName,
};

impl Scopes {
    pub(in crate::infrastructure::mypy) fn enter(
        &mut self,
        kind: Kind,
        body: &[Stmt],
        parameters: BTreeMap<String, Option<Type>>,
        variables: types::Variables,
    ) {
        self.enter_with_bound(kind, body, parameters, variables, &[]);
    }

    pub(in crate::infrastructure::mypy) fn enter_with_bound(
        &mut self,
        kind: Kind,
        body: &[Stmt],
        parameters: BTreeMap<String, Option<Type>>,
        mut variables: types::Variables,
        explicit_bound: &[Type],
    ) {
        self.bind_collection_annotations(&mut variables);
        self.remove_shadowed_builtin_annotations(body, &parameters, &mut variables);
        self.context.typed_dict_forms(body, &mut variables);
        let mut enclosing: std::collections::BTreeSet<_> = self
            .stack
            .iter()
            .flat_map(|scope| scope.bound_variables.iter().copied())
            .collect();
        enclosing.extend(explicit_bound.iter().copied());
        self.context.collect_dataclass_transforms(
            body,
            &self.namespace,
            &mut variables,
            &enclosing,
        );
        self.class_declarations(body, &mut variables);
        self.context.legacy_variables(body, &mut variables);
        self.context.literal_forms(body, &mut variables);
        self.callable_aliases(body, &mut variables);
        let mut bound_variables = explicit_bound.to_vec();
        for &ty in parameters.values().flatten() {
            self.context.collect_variables(ty, &mut bound_variables);
        }
        let bindings = parameters
            .into_iter()
            .map(|(name, ty)| {
                (
                    name,
                    Binding {
                        ty,
                        first_definition: TextSize::new(0),
                        infer_from_assignment: false,
                        callable: None,
                        module: None,
                        explicit: true,
                        module_hidden: Some(false),
                        annotation_form: None,
                        annotation_module: None,
                    },
                )
            })
            .collect();
        let redirects = self.redirects(body);
        let mut overload_aliases = self
            .stack
            .last()
            .map(|scope| scope.overload_aliases.clone())
            .unwrap_or_default();
        let mut imported_names = std::collections::BTreeSet::new();
        for import in crate::infrastructure::project::imports(body, "", false) {
            imported_names.insert(import.local.clone());
            if matches!(import.module.as_str(), "typing" | "typing_extensions") {
                if import.member.as_deref() == Some("overload") {
                    overload_aliases.insert(import.local);
                } else if import.member.is_none() {
                    overload_aliases.insert(format!("{}.overload", import.local));
                }
            }
        }
        let (ordinary_assignment_starts, first_unbound_local_assignments) =
            if kind == Kind::Function {
                OrdinaryAssignments::collect(body)
            } else {
                Default::default()
            };
        self.stack.push(Scope {
            kind,
            module_exports: (kind == Kind::Module).then(|| {
                module_exports::ModuleExports::new(
                    body,
                    self.context.module_target(&self.namespace),
                )
            }),
            bindings,
            direct_function_starts: Default::default(),
            raw_functions: Default::default(),
            named_expression_declarations: Default::default(),
            ordinary_assignment_starts,
            first_unbound_local_assignments,
            comprehension_owner: None,
            walrus_assigned: Default::default(),
            redirects,
            variables,
            redefinitions: imports::import_redefinitions(body),
            overload_aliases,
            bound_variables: bound_variables.into_iter().collect(),
            imported_names,
            imported_functions: BTreeMap::new(),
            import_admission: if kind == Kind::Function {
                imports::import_admission(body, self.context.module_target(&self.namespace))
            } else {
                None
            },
            generator: None,
            super_receiver: None,
        });
        for statement in body {
            match statement {
                Stmt::FunctionDef(function) => {
                    if self
                        .current()
                        .redefinitions
                        .contains(function.name.as_str())
                    {
                        continue;
                    }
                    let signature = self.initial_function_signature(function);
                    self.install_function_declaration(function, signature);
                    self.stack
                        .last_mut()
                        .unwrap()
                        .direct_function_starts
                        .insert(function.name.start());
                }
                Stmt::AnnAssign(assignment) => {
                    if self
                        .prior_inferred_local_definition(&assignment.target, &assignment.annotation)
                        .is_none()
                    {
                        self.declare(&assignment.target, self.annotation(&assignment.annotation));
                    }
                }
                Stmt::ClassDef(class) => self.class_binding(class),
                Stmt::Assign(assignment) => {
                    if self.functional_class_binding(assignment) {
                        continue;
                    }
                    for target in &assignment.targets {
                        self.bind(target, None);
                    }
                }
                _ => {}
            }
        }
        self.predeclare_exception_aliases(body);
        self.bind_imports(body);
        self.refresh_type_aliases(body);
        self.class_namespace_imports(body);
        self.refresh_classes(body);
        self.refresh_type_aliases(body);
        self.refresh_classes(body);
        self.infer_declarations(body);
        if self.refresh_function_decorators(body) {
            self.infer_declarations(body);
        }
        self.class_alias_annotations(body);
        self.install_named_expression_declarations(body);
    }

    fn initial_function_signature(&self, function: &StmtFunctionDef) -> Arc<Signature> {
        let mut signature = Signature::from_function(
            function,
            &self.context,
            &self.live_annotation_variables(&self.current().variables),
            &self.function_binder_input(function, &[]),
        );
        if self.current().kind == Kind::Class {
            self.context.bind_source_self_receiver(
                &mut signature,
                function,
                super::super::classes::method_kind(function),
            );
        }
        if self.current().kind == Kind::Module {
            let module = self
                .namespace
                .split_once(':')
                .map_or(self.namespace.as_str(), |(_, module)| module);
            if !matches!(module, "builtins" | "typing") {
                signature.definition_module = Some(module.to_owned());
            }
        }
        self.capture_function_variables(self.overload_signature(function, signature))
    }

    pub(in crate::infrastructure::mypy) fn register_compound_function(
        &mut self,
        function: &StmtFunctionDef,
    ) -> Option<Type> {
        let scope = self.current();
        if scope
            .direct_function_starts
            .contains(&function.name.start())
            || scope.redefinitions.contains(function.name.as_str())
            || scope.redirects.contains_key(function.name.as_str())
        {
            return None;
        }
        if let Some(binding) = scope.bindings.get(function.name.as_str()) {
            if binding.first_definition == function.name.start()
                && scope.raw_functions.contains_key(&function.name.start())
            {
                return binding.ty;
            }
        }
        let signature = self.initial_function_signature(function);
        let signature = self.capture_function_variables(Arc::new(
            self.refresh_function_annotations(function, &signature),
        ));
        self.install_function_declaration(function, signature);
        self.apply_function_decorators(function);
        self.current().bindings.get(function.name.as_str())?.ty
    }

    pub(in crate::infrastructure::mypy) fn function_declaration_signature(
        &self,
        function: &StmtFunctionDef,
    ) -> Option<Arc<Signature>> {
        let scope = self.current();
        let binding = scope.bindings.get(function.name.as_str())?;
        if binding.first_definition != function.name.start() {
            return None;
        }
        scope.raw_functions.get(&function.name.start()).cloned()
    }
}

// The existing statement-target map stays separate from the new admission proof.
#[derive(Default)]
struct OrdinaryAssignments {
    starts: BTreeMap<TextSize, bool>,
    first: BTreeMap<String, TextSize>,
    blocked: std::collections::BTreeSet<String>,
    unsupported_proof: bool,
}

impl OrdinaryAssignments {
    fn collect(body: &[Stmt]) -> (BTreeMap<TextSize, bool>, BTreeMap<String, TextSize>) {
        let mut targets = Self::default();
        for statement in body {
            targets.visit_stmt(statement);
        }
        let first = if targets.unsupported_proof {
            BTreeMap::new()
        } else {
            targets
                .first
                .into_iter()
                .filter(|(name, _)| !targets.blocked.contains(name))
                .collect()
        };
        (targets.starts, first)
    }

    // This proves only absence of a header Name binder. It does not evaluate
    // annotations/decorators or make a claim about runtime callable effects.
    fn read_only_header(expression: &Expr) -> bool {
        match expression {
            Expr::Name(name) => name.ctx.is_load(),
            Expr::StringLiteral(_)
            | Expr::BytesLiteral(_)
            | Expr::NumberLiteral(_)
            | Expr::BooleanLiteral(_)
            | Expr::NoneLiteral(_)
            | Expr::EllipsisLiteral(_) => true,
            Expr::Attribute(attribute) => {
                attribute.ctx.is_load() && Self::read_only_header(&attribute.value)
            }
            Expr::Subscript(subscript) => {
                subscript.ctx.is_load()
                    && Self::read_only_header(&subscript.value)
                    && Self::read_only_header(&subscript.slice)
            }
            Expr::Tuple(tuple) => tuple.elts.iter().all(Self::read_only_header),
            _ => false,
        }
    }

    fn record_target(&mut self, target: &Expr, allow_unknown: bool) {
        match target {
            Expr::Name(name) => {
                self.starts.insert(name.start(), allow_unknown);
                if allow_unknown {
                    self.first
                        .entry(name.id.to_string())
                        .and_modify(|first| *first = (*first).min(name.start()))
                        .or_insert(name.start());
                } else {
                    self.blocked.insert(name.id.to_string());
                }
            }
            Expr::Tuple(tuple) => {
                for target in &tuple.elts {
                    self.record_target(target, false);
                }
            }
            Expr::List(list) => {
                for target in &list.elts {
                    self.record_target(target, false);
                }
            }
            _ => self.visit_expr(target),
        }
    }
}

impl<'a> Visitor<'a> for OrdinaryAssignments {
    fn visit_stmt(&mut self, statement: &'a Stmt) {
        match statement {
            Stmt::Assign(assignment) => {
                for target in &assignment.targets {
                    self.record_target(target, true);
                }
                self.visit_expr(&assignment.value);
            }
            Stmt::AnnAssign(assignment) => {
                self.record_target(&assignment.target, false);
                walk_stmt(self, statement);
            }
            Stmt::For(looped) => {
                self.record_target(&looped.target, false);
                walk_stmt(self, statement);
            }
            Stmt::Import(_) | Stmt::ImportFrom(_) => {
                for import in crate::infrastructure::project::imports(
                    std::slice::from_ref(statement),
                    "",
                    false,
                ) {
                    if import.member.as_deref() == Some("*") {
                        self.unsupported_proof = true;
                    }
                    self.blocked.insert(import.local);
                }
            }
            Stmt::Global(declaration) => {
                self.blocked
                    .extend(declaration.names.iter().map(ToString::to_string));
            }
            Stmt::Nonlocal(declaration) => {
                self.blocked
                    .extend(declaration.names.iter().map(ToString::to_string));
            }
            Stmt::FunctionDef(function) => {
                self.blocked.insert(function.name.to_string());
                self.unsupported_proof |= function.type_params.is_some()
                    || function
                        .decorator_list
                        .iter()
                        .any(|decorator| !Self::read_only_header(&decorator.expression))
                    || function
                        .returns
                        .as_deref()
                        .is_some_and(|annotation| !Self::read_only_header(annotation))
                    || function.parameters.iter().any(|parameter| {
                        parameter
                            .annotation()
                            .is_some_and(|annotation| !Self::read_only_header(annotation))
                    });
                for parameter in function.parameters.iter_non_variadic_params() {
                    if let Some(default) = parameter.default.as_deref() {
                        self.unsupported_proof |= !Self::read_only_header(default);
                    }
                }
                // The body and its parameter binders have a different owner.
            }
            Stmt::ClassDef(_) => self.unsupported_proof = true,
            Stmt::Match(_) | Stmt::TypeAlias(_) => {
                self.unsupported_proof = true;
                walk_stmt(self, statement);
            }
            _ => walk_stmt(self, statement),
        }
    }

    fn visit_expr(&mut self, expression: &'a Expr) {
        match expression {
            Expr::Name(name) if !name.ctx.is_load() => {
                self.blocked.insert(name.id.to_string());
            }
            Expr::Lambda(lambda) => {
                if let Some(parameters) = &lambda.parameters {
                    for parameter in parameters.iter_non_variadic_params() {
                        if let Some(default) = parameter.default.as_deref() {
                            self.unsupported_proof |= !Self::read_only_header(default);
                        }
                    }
                }
                // Only defaults execute here. Do not walk the lambda body.
            }
            Expr::ListComp(_) | Expr::SetComp(_) | Expr::DictComp(_) | Expr::Generator(_) => {
                self.unsupported_proof = true;
            }
            _ => walk_expr(self, expression),
        }
    }

    fn visit_except_handler(&mut self, handler: &'a ExceptHandler) {
        let ExceptHandler::ExceptHandler(value) = handler;
        if let Some(name) = &value.name {
            self.blocked.insert(name.to_string());
        }
        walk_except_handler(self, handler);
    }
}

impl Scopes {
    pub(super) fn earlier_local_assignment(&self, name: &ExprName) -> bool {
        let scope = self.current();
        scope.kind == Kind::Function
            && !scope.redirects.contains_key(name.id.as_str())
            && scope.ordinary_assignment_starts.contains_key(&name.start())
            && scope.bindings.get(name.id.as_str()).is_some_and(|binding| {
                binding.infer_from_assignment && name.start() < binding.first_definition
            })
    }

    pub(in crate::infrastructure::mypy) fn unavailable_inferred_local(
        &self,
        name: &ExprName,
    ) -> bool {
        let scope = self.current();
        if name.ctx.is_load() && scope.walrus_assigned.get(name.id.as_str()) == Some(&false) {
            // A pending store does not erase an explicit declaration's known type.
            return scope
                .comprehension_owner
                .filter(|&owner| {
                    self.stack[owner].kind == Kind::Function
                        && scope.redirects.get(name.id.as_str()) == Some(&owner)
                })
                .and_then(|owner| self.stack[owner].bindings.get(name.id.as_str()))
                .is_some_and(|binding| binding.infer_from_assignment);
        }
        name.ctx.is_load()
            && scope.kind == Kind::Function
            && !scope.redirects.contains_key(name.id.as_str())
            && scope.bindings.get(name.id.as_str()).is_some_and(|binding| {
                binding.infer_from_assignment
                    && scope
                        .ordinary_assignment_starts
                        .contains_key(&binding.first_definition)
                    && name.start() < binding.first_definition
            })
    }

    pub(in crate::infrastructure::mypy) fn inference_environment(
        &self,
        expression: &Expr,
        mut environment: BTreeMap<String, Type>,
    ) -> BTreeMap<String, Type> {
        let mut reads = ForwardReads {
            scopes: self,
            names: Default::default(),
        };
        reads.visit_expr(expression);
        for name in reads.names {
            // Recovery belongs to this read, not to the future declaration.
            environment.insert(name, Type::Any);
        }
        environment
    }

    pub(super) fn inference_environment_view<'a>(
        &self,
        expression: &Expr,
        environment: &'a BTreeMap<String, Type>,
    ) -> std::borrow::Cow<'a, BTreeMap<String, Type>> {
        let mut reads = ForwardReads {
            scopes: self,
            names: Default::default(),
        };
        reads.visit_expr(expression);
        if reads
            .names
            .iter()
            .all(|name| environment.get(name).is_some_and(|ty| ty.is_any()))
        {
            return std::borrow::Cow::Borrowed(environment);
        }
        let mut environment = environment.clone();
        for name in reads.names {
            // Keep corrections local to this read, as in the owned API.
            environment.insert(name, Type::Any);
        }
        std::borrow::Cow::Owned(environment)
    }
}

struct ForwardReads<'s> {
    scopes: &'s Scopes,
    names: std::collections::BTreeSet<String>,
}

impl<'a> Visitor<'a> for ForwardReads<'_> {
    fn visit_expr(&mut self, expression: &'a Expr) {
        match expression {
            Expr::Name(name) if self.scopes.unavailable_inferred_local(name) => {
                self.names.insert(name.id.to_string());
            }
            Expr::Lambda(lambda) => {
                // Defaults execute here; the body has its own lexical scope.
                if let Some(parameters) = &lambda.parameters {
                    for parameter in parameters.iter_non_variadic_params() {
                        if let Some(default) = parameter.default.as_deref() {
                            self.visit_expr(default);
                        }
                    }
                }
            }
            Expr::Generator(_) | Expr::ListComp(_) | Expr::SetComp(_) | Expr::DictComp(_) => {
                // Only the first iterator executes in this enclosing scope.
                let generators: &[_] = match expression {
                    Expr::Generator(value) => &value.generators,
                    Expr::ListComp(value) => &value.generators,
                    Expr::SetComp(value) => &value.generators,
                    Expr::DictComp(value) => &value.generators,
                    _ => unreachable!(),
                };
                if let Some(first) = generators.first() {
                    self.visit_expr(&first.iter);
                }
            }
            _ => walk_expr(self, expression),
        }
    }
}
