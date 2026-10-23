//! Class declarations share identities between module exports and local checking.
use super::*;
use mamba_language::application::quality_syntax::python_ast::{StmtAssign, StmtClassDef};
mod base_provenance;

impl Scopes {
    pub(in crate::infrastructure::mypy) fn complete_embedded_ast_constant(&self) {
        let Some(id) = self
            .current()
            .variables
            .get("Constant")
            .and_then(|&ty| self.context.class_id(ty))
        else {
            return;
        };
        // Preserve the existing typed Constant boundary on the source class.
        // Its expr/AST bases remain the declarations from this module scope.
        self.context
            .populate_ast_constant(id, self.context.module_target(&self.namespace));
    }

    pub(super) fn class_alias_annotations(&mut self, body: &[Stmt]) {
        for statement in body {
            let Stmt::Assign(assignment) = statement else {
                continue;
            };
            if self.alias_definition(statement) {
                continue;
            }
            if !matches!(
                assignment.value.as_ref(),
                Expr::Name(_) | Expr::Attribute(_)
            ) {
                continue;
            }
            let Some(ty) = self.annotation(&assignment.value) else {
                continue;
            };
            if !matches!(
                ty,
                Type::Any
                    | Type::AnySpecial
                    | Type::Int
                    | Type::Float
                    | Type::Complex
                    | Type::Str
                    | Type::Bytes
                    | Type::Bool
                    | Type::Object
            ) && self.context.class_id(ty).is_none()
            {
                continue;
            }
            for target in &assignment.targets {
                if let Expr::Name(name) = target {
                    self.stack
                        .last_mut()
                        .unwrap()
                        .variables
                        .insert(name.id.to_string(), ty);
                }
            }
        }
    }

    pub(in crate::infrastructure::mypy) fn qualified_name(&self, name: &str) -> String {
        let module = self
            .namespace
            .split_once(':')
            .map_or(self.namespace.as_str(), |(_, name)| name);
        format!("{module}.{name}")
    }

    pub(super) fn class_declarations(&self, body: &[Stmt], variables: &mut types::Variables) {
        let redefinitions = super::imports::import_redefinitions(body);
        for statement in body {
            let (name, offset) = match statement {
                Stmt::ClassDef(class) if !redefinitions.contains(class.name.as_str()) => {
                    (class.name.as_str(), class.name.start())
                }
                Stmt::Assign(assignment) => {
                    let ([Expr::Name(name)], Expr::Call(call)) =
                        (assignment.targets.as_slice(), assignment.value.as_ref())
                    else {
                        continue;
                    };
                    if self.context.typing_form(&call.func, variables).as_deref()
                        != Some("TypedDict")
                    {
                        continue;
                    }
                    (name.id.as_str(), name.start())
                }
                _ => continue,
            };
            let id = self
                .context
                .declare_class(&self.namespace, name, offset.to_u32());
            variables.insert(name.to_string(), self.context.class(id).instance);
        }
        self.refresh_typed_dicts(body, variables);
    }

    fn refresh_typed_dicts(&self, body: &[Stmt], variables: &types::Variables) {
        for statement in body {
            let (name, offset, definition) = match statement {
                Stmt::ClassDef(class) => (
                    class.name.as_str(),
                    class.name.start(),
                    self.context.class_typed_dict(class, variables),
                ),
                Stmt::Assign(assignment) => {
                    let ([Expr::Name(name)], Expr::Call(call)) =
                        (assignment.targets.as_slice(), assignment.value.as_ref())
                    else {
                        continue;
                    };
                    (
                        name.id.as_str(),
                        name.start(),
                        self.context.functional_typed_dict(call, variables),
                    )
                }
                _ => continue,
            };
            let Some(definition) = definition else {
                continue;
            };
            let id = self
                .context
                .declare_class(&self.namespace, name, offset.to_u32());
            let mut class = (*self.context.class(id)).clone();
            class.typed_dict = Some(definition);
            self.context.replace_class(id, class);
        }
    }

    pub(in crate::infrastructure::mypy) fn functional_class_binding(
        &mut self,
        assignment: &StmtAssign,
    ) -> bool {
        let ([Expr::Name(name)], Expr::Call(call)) =
            (assignment.targets.as_slice(), assignment.value.as_ref())
        else {
            return false;
        };
        if self
            .context
            .functional_typed_dict(
                call,
                &self.live_annotation_variables(&self.current().variables),
            )
            .is_none()
        {
            return false;
        }
        let id =
            self.context
                .declare_class(&self.namespace, name.id.as_str(), name.start().to_u32());
        self.stack.last_mut().unwrap().bindings.insert(
            name.id.to_string(),
            Binding {
                ty: Some(self.context.class(id).object),
                first_definition: name.start(),
                infer_from_assignment: false,
                callable: None,
                module: None,
                explicit: true,
                module_hidden: Some(false),
                annotation_form: None,
                annotation_module: None,
            },
        );
        true
    }

    pub(super) fn class_binding(&mut self, class: &StmtClassDef) {
        if self.import_redefinition(class.name.as_str()) {
            return;
        }
        let id = self.context.declare_class(
            &self.namespace,
            class.name.as_str(),
            class.name.start().to_u32(),
        );
        let ty = self.context.class(id).object;
        self.stack.last_mut().unwrap().bindings.insert(
            class.name.to_string(),
            Binding {
                ty: Some(ty),
                first_definition: class.name.start(),
                infer_from_assignment: false,
                callable: None,
                module: None,
                explicit: true,
                module_hidden: Some(false),
                annotation_form: None,
                annotation_module: None,
            },
        );
    }

    pub(super) fn refresh_classes(&mut self, body: &[Stmt]) {
        self.refresh_typed_dicts(
            body,
            &self.live_annotation_variables(&self.current().variables),
        );
        for statement in body {
            if let Stmt::FunctionDef(function) = statement {
                if self.is_overload(function) {
                    continue;
                }
                if let Some(signature) = self.function_declaration_signature(function) {
                    let signature = self.capture_function_variables(Arc::new(
                        self.refresh_function_annotations(function, &signature),
                    ));
                    self.replace_raw_function_signature(function.name.as_str(), signature);
                }
            }
            if let Stmt::ClassDef(class) = statement {
                if self.import_redefinition(class.name.as_str()) {
                    continue;
                }
                let id = self.context.declare_class(
                    &self.namespace,
                    class.name.as_str(),
                    class.name.start().to_u32(),
                );
                let missing_bases = self.missing_class_base_members(class);
                let generic_bases = base_provenance::generic_bases(self, body, class);
                let metadata = {
                    let enclosing = self.source_method_enclosing_bounds();
                    let annotation_proof = self.class_method_annotation_proof(class);
                    let method_input = |owner,
                                        definition: &StmtClassDef,
                                        function: &StmtFunctionDef,
                                        variables: &types::Variables,
                                        generic: &[Type]| {
                        self.method_binder_input(
                            function,
                            (owner, definition, generic),
                            variables,
                            enclosing.as_ref(),
                            &|function, variables| {
                                if !std::ptr::eq(class, definition) {
                                    return Err((
                                        function.start().to_u32(),
                                        "method collector supplied a different class AST",
                                    ));
                                }
                                annotation_proof(function, variables)
                            },
                        )
                    };
                    self.context.populate_class_with_method_inputs(
                        id,
                        class,
                        &self.class_field_annotation_variables(class),
                        &self.current().overload_aliases,
                        (&missing_bases, &generic_bases),
                        (self.terminal_none_source, Some(&method_input)),
                    )
                };
                self.refresh_class_decorators(id, class);
                self.refresh_constructor_fields(id, class, &metadata);
            }
        }
        for (name, model) in super::super::pydantic::collect(
            body,
            &self.context,
            &self.live_annotation_variables(&self.current().variables),
        ) {
            if let Some(id) = self
                .current()
                .variables
                .get(&name)
                .and_then(|&ty| self.context.class_id(ty))
            {
                let mut class = (*self.context.class(id)).clone();
                class.pydantic = Some(model);
                self.context.replace_class(id, class);
            }
        }
        self.refresh_overloads(body);
    }

    fn missing_class_base_members(
        &self,
        class: &StmtClassDef,
    ) -> BTreeMap<u32, super::super::classes::AnyBaseOrigin> {
        let mut missing = BTreeMap::new();
        let (Some(arguments), Some(lookup)) = (&class.arguments, &self.modules) else {
            return missing;
        };
        for base in &arguments.args {
            // Other annotation failures are not proof of an original Any base.
            if self
                .context
                .class_base_annotation(
                    base,
                    &self.live_annotation_variables(&self.current().variables),
                )
                .is_some()
            {
                continue;
            }
            let Expr::Attribute(attribute) = base else {
                continue;
            };
            let Expr::Name(root) = attribute.value.as_ref() else {
                continue;
            };
            let Some(module) = self
                .resolved_binding(root.id.as_str())
                .and_then(|binding| binding.module.as_deref())
            else {
                continue;
            };
            // With no __all__, this existing query checks every declared global
            // has a native export. Its Err/None results preserve provisional state.
            if !matches!(lookup.source_public_names(module), Ok(Some((false, _)))) {
                continue;
            }
            let Some(exports) = lookup.exports(module) else {
                continue;
            };
            let member = attribute.attr.as_str();
            if exports.contains_key(member)
                || exports.contains_key("__getattr__")
                || lookup.member(module, member).is_some()
            {
                continue;
            }
            missing.insert(
                base.start().to_u32(),
                super::super::classes::AnyBaseOrigin::MissingModuleMember {
                    range: base.range(),
                    module: module.to_owned(),
                    member: member.to_owned(),
                },
            );
        }
        missing
    }

    fn refresh_constructor_fields(
        &mut self,
        id: u32,
        class: &StmtClassDef,
        metadata: &super::super::classes::CollectionMetadata,
    ) {
        let Some(constructor_offset) = metadata.constructor_offset else {
            return;
        };
        let target = self.context.module_target(&self.namespace);
        let Some(function) = super::super::classes::statements::collect(&class.body, target)
            .into_iter()
            .find_map(|statement| match statement {
                Stmt::FunctionDef(function) if function.name.start() == constructor_offset => {
                    Some(function)
                }
                _ => None,
            })
        else {
            return;
        };
        let Some(method) = self.context.class(id).methods.get("__init__").cloned() else {
            return;
        };
        if method.kind != super::super::classes::MethodKind::Instance
            || method
                .signature
                .parameters
                .first()
                .and_then(|parameter| parameter.ty)
                .is_none_or(|ty| {
                    self.context.class_id(ty) != Some(id) || self.context.is_class_value(ty)
                })
            || !self
                .context
                .class(id)
                .fields
                .values()
                .any(|field| field.ty.is_none() && !field.class_access && !field.class_var)
        {
            return;
        }

        let signature = method.signature;
        let parameters: BTreeMap<String, Option<Type>> = signature
            .parameters
            .iter()
            .map(|parameter| (parameter.name.clone(), parameter.ty))
            .collect();
        let super_receiver = function
            .parameters
            .iter()
            .next()
            .and_then(|parameter| {
                if parameter.is_variadic() {
                    None
                } else {
                    parameters.get(parameter.name().as_str()).copied().flatten()
                }
            })
            .map(|receiver| (id, receiver));
        let mut constructor_bound = signature.generic.to_vec();
        constructor_bound.extend(self.context.class(id).generic.iter().copied());
        self.enter_with_bound(
            Kind::Function,
            &function.body,
            parameters,
            signature.variables.clone(),
            &constructor_bound,
        );
        self.set_super_receiver(super_receiver);
        let environment = self.environment();
        let context = self.context.clone();
        let call_resolver = |expression: &Expr, environment: &BTreeMap<String, Type>| {
            let environment = self.inference_environment_view(expression, environment);
            self.context
                .infer_with_calls(expression, &environment, &|call| {
                    self.expression_type(call, &environment)
                })
        };
        context.populate_constructor_fields(
            id,
            function,
            &signature.variables,
            &environment,
            metadata,
            &call_resolver,
        );
        self.leave();
    }

    pub(in crate::infrastructure::mypy) fn constructor_none_plan(
        &self,
        id: u32,
        class: &StmtClassDef,
        sha256: &str,
    ) -> Option<super::super::classes::ConstructorNonePlan> {
        self.none_constructor_source(id, class)
            .then(|| self.context.constructor_none_plan(id, class, sha256))
            .flatten()
    }

    pub(in crate::infrastructure::mypy) fn constructor_none_scope(
        &self,
        plan: &super::super::classes::ConstructorNonePlan,
    ) -> bool {
        let class = self.context.class(plan.id);
        self.stack.len() == 3
            && self.stack[0].kind == Kind::Module
            && self.stack[1].kind == Kind::Class
            && self.current().kind == Kind::Function
            && self.current().comprehension_owner.is_none()
            && self.current().super_receiver == Some((plan.id, plan.instance))
            && self.namespace == class.namespace
            && self.current().redirects.is_empty()
            && self
                .current()
                .bindings
                .get(&plan.receiver)
                .is_some_and(|binding| {
                    binding.ty == Some(plan.instance)
                        && binding.explicit
                        && !binding.infer_from_assignment
                        && binding.first_definition == TextSize::new(0)
                })
            && self.stack[0]
                .bindings
                .get(&class.name)
                .is_some_and(|binding| {
                    binding.ty == Some(class.object)
                        && binding.first_definition.to_u32() == class.offset
                        && binding.module.is_none()
                        && binding.callable.is_none()
                })
    }

    fn none_constructor_source(&self, id: u32, class: &StmtClassDef) -> bool {
        let Some(original) = &self.original_builtin_roots else {
            return false;
        };
        let Some(binding) = self.current().bindings.get(class.name.as_str()) else {
            return false;
        };
        self.terminal_none_source
            && self.stack.len() == 1
            && self.current().kind == Kind::Module
            && original.namespace == self.namespace
            && original.target == self.context.module_target(&self.namespace)
            && original.roots.contains(class.name.as_str())
            && original.global_hazards.is_empty()
            && !self.current().redefinitions.contains(class.name.as_str())
            && !self.current().imported_names.contains(class.name.as_str())
            && !binding.infer_from_assignment
            && binding.module.is_none()
            && binding.callable.is_none()
            && binding.first_definition == class.name.start()
            && binding.ty == Some(self.context.class(id).object)
            && self.context.class(id).namespace == self.namespace
            && self.context.class(id).offset == class.name.start().to_u32()
    }

    pub(in crate::infrastructure::mypy) fn class_object(&self, name: &str) -> Option<Type> {
        let ty = self.resolved_binding(name)?.ty?;
        self.context.is_class_value(ty).then_some(ty)
    }

    pub(super) fn class_namespace_imports(&mut self, body: &[Stmt]) {
        let mut names = self.live_annotation_variables(&self.current().variables);
        for (name, binding) in &self.current().bindings {
            if let Some(ty) = binding.ty.filter(|&ty| self.context.is_class_value(ty)) {
                names.insert(name.clone(), self.context.annotation_value(ty));
            }
            if let Some(module) = binding.module.as_deref() {
                if let Some(exports) = self
                    .modules
                    .as_ref()
                    .and_then(|lookup| lookup.exports(module))
                {
                    for (member, export) in exports {
                        if let Some(alias) = export.alias {
                            if !self.context.is_callable_form(alias)
                                || self.live_annotation_module(binding).is_some()
                            {
                                names.insert(format!("{name}.{member}"), alias);
                            }
                        }
                        if let Some(ty) = export.ty.filter(|&ty| self.context.is_class_value(ty)) {
                            names.insert(
                                format!("{name}.{member}"),
                                self.context.annotation_value(ty),
                            );
                        }
                    }
                } else if let Some(lookup) = &self.modules {
                    if !lookup.module_exists(module) {
                        for (member, ty) in lookup.native_class_annotations(module) {
                            names.insert(format!("{name}.{member}"), ty);
                        }
                    }
                }
            }
        }
        if let Some(lookup) = &self.modules {
            for name in lookup.annotation_paths(body, names.as_map()) {
                if let Some(form) = self.live_callable_form_path(&name) {
                    names.insert(name, form);
                    continue;
                }
                if names.contains_key(&name) {
                    continue;
                }
                let (root, path) = name.split_once('.').unwrap();
                if let Some(module) = self
                    .resolved_binding(root)
                    .and_then(|binding| binding.module.as_deref())
                {
                    if let Some(ty) = lookup.module_annotation(module, path) {
                        if !self.context.is_callable_form(ty) {
                            names.insert(name, ty);
                        }
                    }
                }
            }
        }
        self.stack.last_mut().unwrap().variables = names;
    }
}
