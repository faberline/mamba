//! Value names share lexical lookup, pinned builtins, and implicit module values.
use super::*;
use mamba_language::application::quality_syntax::python_ast::{
    visitor::{walk_expr, walk_stmt, Visitor},
    Comprehension, ExprName,
};
use std::collections::BTreeSet;

// This reads source syntax only. It does not visit/check values or infer types.
#[derive(Default)]
struct NamedExpressionAnnotations {
    annotations: BTreeMap<String, Option<TextSize>>,
    writes: BTreeSet<String>,
    unsupported: bool,
}

impl<'a> Visitor<'a> for NamedExpressionAnnotations {
    fn visit_stmt(&mut self, statement: &'a Stmt) {
        match statement {
            // These declarations need more symbol/annotation provenance.
            Stmt::FunctionDef(_)
            | Stmt::ClassDef(_)
            | Stmt::Import(_)
            | Stmt::ImportFrom(_)
            | Stmt::TypeAlias(_)
            | Stmt::Match(_) => {
                self.unsupported = true;
                return;
            }
            Stmt::AnnAssign(assignment) => {
                if let Expr::Name(name) = assignment.target.as_ref() {
                    self.annotations
                        .entry(name.id.to_string())
                        .and_modify(|previous| *previous = None)
                        .or_insert(Some(name.start()));
                }
            }
            _ => {}
        }
        walk_stmt(self, statement);
    }

    fn visit_expr(&mut self, expression: &'a Expr) {
        if let Expr::Name(name) = expression {
            if !name.ctx.is_load() {
                self.writes.insert(name.id.to_string());
            }
        }
        walk_expr(self, expression);
    }
}

impl Scopes {
    pub(super) fn install_named_expression_declarations(&mut self, body: &[Stmt]) {
        let scope = self.current();
        let Some(source) = &self.original_builtin_roots else {
            return;
        };
        if scope.kind != Kind::Function
            || scope.comprehension_owner.is_some()
            || !self.terminal_none_source
            || source.namespace != self.namespace
            || source.target != self.context.module_target(&self.namespace)
            || !self
                .modules
                .as_ref()
                .is_some_and(|lookup| lookup.namespace() == self.namespace)
        {
            return;
        }
        let mut syntax = NamedExpressionAnnotations::default();
        for statement in body {
            syntax.visit_stmt(statement);
        }
        if syntax.unsupported {
            return;
        }
        let owner = self.stack.len() - 1;
        let mut declarations = BTreeMap::new();
        for statement in body {
            let Stmt::AnnAssign(assignment) = statement else {
                continue;
            };
            let (Expr::Name(name), Expr::Name(annotation)) =
                (assignment.target.as_ref(), assignment.annotation.as_ref())
            else {
                continue;
            };
            // The original AST must name an unshadowed builtin directly.
            // Final, aliases, strings, unions and contextual forms never qualify.
            let (annotation, expected) = match annotation.id.as_str() {
                "int" => ("int", Type::Int),
                "float" => ("float", Type::Float),
                "str" => ("str", Type::Str),
                "bytes" => ("bytes", Type::Bytes),
                "bool" => ("bool", Type::Bool),
                _ => continue,
            };
            if name.id.as_str().starts_with("__")
                || syntax.annotations.get(name.id.as_str()) != Some(&Some(name.start()))
                || syntax.writes.contains(annotation)
                || source.roots.contains(annotation)
                || source.global_hazards.contains(annotation)
                || self.has_binding(annotation)
                || scope.variables.contains_key(annotation)
                || scope.redirects.contains_key(name.id.as_str())
                || scope.imported_names.contains(name.id.as_str())
                || scope.redefinitions.contains(name.id.as_str())
            {
                continue;
            }
            let Some(binding) = scope.bindings.get(name.id.as_str()) else {
                continue;
            };
            if binding.first_definition != name.start()
                || binding.infer_from_assignment
                || binding.ty != Some(expected)
                || binding.module.is_some()
                || binding.callable.is_some()
                || !binding.explicit
            {
                continue;
            }
            declarations.insert(
                name.id.to_string(),
                NamedExpressionDeclaration {
                    owner,
                    annotation,
                    origin: binding.clone(),
                },
            );
        }
        // No write mutates these original declaration facts. Scope clones share
        // them, and leaving/re-entering a scope creates a different map.
        self.stack.last_mut().unwrap().named_expression_declarations = Arc::new(declarations);
    }

    pub(in crate::infrastructure::mypy) fn named_expression_declared_type(
        &self,
        target: &Expr,
    ) -> Option<Type> {
        let Expr::Name(name) = target else {
            return None;
        };
        let scope = self.current();
        if scope.kind != Kind::Function
            || scope.comprehension_owner.is_some()
            || scope.redirects.contains_key(name.id.as_str())
        {
            return None;
        }
        let declaration = scope.named_expression_declarations.get(name.id.as_str())?;
        let binding = scope.bindings.get(name.id.as_str())?;
        (declaration.owner + 1 == self.stack.len()
            && declaration.origin.first_definition < name.start()
            && *binding == declaration.origin
            && self
                .resolved_binding(name.id.as_str())
                .is_some_and(|resolved| std::ptr::eq(resolved, binding))
            && !self.has_binding(declaration.annotation)
            && !scope.variables.contains_key(declaration.annotation))
        .then_some(binding.ty)
        .flatten()
    }

    pub(in crate::infrastructure::mypy) fn known_name(&self, name: &str) -> bool {
        self.has_binding(name)
            || self
                .current()
                .variables
                .get(name)
                .and_then(|&ty| self.context.node(ty))
                .is_some_and(|node| matches!(node.as_ref(), types::Node::Variable(_)))
            || self.stack.iter().any(|scope| {
                scope.bound_variables.iter().any(|&ty| {
                    self.context.node(ty).is_some_and(|node| {
                        matches!(node.as_ref(),
                    types::Node::Variable(variable) if variable.name == name)
                    })
                })
            })
            || self.implicit_module_value(name).is_some()
            || matches!(name, "reveal_type" | "reveal_locals")
            || (!name.starts_with('_') || name.starts_with("__"))
                && self
                    .context
                    .name_builtin_names(self.context.module_target(&self.namespace))
                    .contains(name)
    }

    pub(super) fn name_expression_type(&self, name: &ExprName) -> Option<Type> {
        if self.has_binding(name.id.as_str()) {
            return None;
        }
        let expression = Expr::Name(name.clone());
        self.builtin_value_expression(&expression)
            .or_else(|| self.implicit_module_value(name.id.as_str()))
            .or_else(|| (!self.known_name(name.id.as_str())).then_some(Type::Any))
    }

    fn implicit_module_value(&self, name: &str) -> Option<Type> {
        let is_package = self
            .modules
            .as_ref()
            .is_some_and(|lookup| lookup.is_package());
        self.context.module_global(name, is_package)
    }

    pub(super) fn install_module_globals(&mut self) {
        for name in [
            "__name__",
            "__doc__",
            "__file__",
            "__package__",
            "__annotations__",
            "__spec__",
            "__path__",
        ] {
            let Some(ty) = self.implicit_module_value(name) else {
                continue;
            };
            {
                self.stack.last_mut().unwrap().bindings.insert(
                    name.into(),
                    Binding {
                        ty: Some(ty),
                        first_definition: TextSize::new(0),
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
        }
    }

    pub(in crate::infrastructure::mypy) fn redefined_module_global(&self, target: &Expr) -> bool {
        self.current().kind == Kind::Module
            && matches!(target,
            Expr::Name(name) if self.implicit_module_value(name.id.as_str()).is_some())
    }

    pub(in crate::infrastructure::mypy) fn undefined_name_message(
        &self,
        name: &str,
    ) -> (String, Option<String>) {
        let target = self.context.module_target(&self.namespace);
        let mut message = format!("Name \"{name}\" is not defined");
        let hint = self.context.unimported_name_hint(name, target);
        if !(name.starts_with("__") && name.ends_with("__")) {
            let mut names: BTreeSet<_> = self
                .context
                .name_builtin_names(target)
                .iter()
                .filter(|name| !name.starts_with('_') || name.starts_with("__"))
                .map(String::as_str)
                .collect();
            for (index, scope) in self.stack.iter().enumerate() {
                if scope.kind != Kind::Class || index + 1 == self.stack.len() {
                    names.extend(scope.bindings.keys().map(String::as_str));
                }
            }
            names.remove(name);
            let matches = super::super::signatures::suggestions::best_matches(name, &names);
            let quoted: Vec<_> = matches.iter().map(|name| format!("\"{name}\"")).collect();
            let suggestion = match quoted.as_slice() {
                [] => None,
                [one] => Some(one.clone()),
                [first, second] => Some(format!("{first} or {second}")),
                many => Some(format!(
                    "{}, or {}",
                    many[..many.len() - 1].join(", "),
                    many.last().unwrap()
                )),
            };
            if let Some(suggestion) = suggestion {
                message.push_str(&format!("; did you mean {suggestion}?"));
            }
        }
        (message, hint)
    }

    pub(super) fn comprehension_scope(
        &self,
        generators: &[Comprehension],
        environment: &BTreeMap<String, Type>,
    ) -> Option<(Self, BTreeMap<String, Type>)> {
        self.comprehension_scope_with_item(
            generators,
            environment,
            &|ty| self.context.iterable_item(ty),
            false,
        )
    }

    pub(super) fn comprehension_scope_with_item(
        &self,
        generators: &[Comprehension],
        initial: &BTreeMap<String, Type>,
        iterable_item: &impl Fn(Type) -> Option<Type>,
        generator_expression: bool,
    ) -> Option<(Self, BTreeMap<String, Type>)> {
        let mut scope = self.clone();
        scope.enter_comprehension(generators);
        let mut environment = scope.environment();
        for (key, &ty) in initial {
            let name = key.split('.').next().unwrap();
            if !scope.current().bindings.contains_key(name)
                && self
                    .resolved_binding(name)
                    .map(|binding| binding.first_definition)
                    == scope
                        .resolved_binding(name)
                        .map(|binding| binding.first_definition)
            {
                environment.insert(key.clone(), ty);
            }
        }
        for (index, generator) in generators.iter().enumerate() {
            // Python evaluates only the first iterator in the enclosing scope.
            let resolver = if index == 0 { self } else { &scope };
            let inputs = if index == 0 { initial } else { &environment };
            let actual = self
                .context
                .infer_with_calls(&generator.iter, inputs, &|value| {
                    resolver.expression_type(value, inputs)
                });
            // Keep each expression kind's existing unknown-iterator recovery.
            let item = if generator_expression {
                iterable_item(actual?).unwrap_or(Type::Any)
            } else {
                actual.and_then(iterable_item).unwrap_or(Type::Any)
            };
            let bindings = if generator_expression {
                let mut bindings = BTreeMap::new();
                scope.comprehension_target(&generator.target, item, &mut bindings);
                scope.bind_comprehension_bindings(&bindings);
                bindings
            } else {
                scope.bind_comprehension_target(&generator.target, item)
            };
            for name in bindings.keys() {
                Self::clear_comprehension_binding(&mut environment, name);
            }
            environment.extend(bindings);
            for condition in &generator.ifs {
                let previous = environment.clone();
                if !scope.refine_comprehension_filter(condition, true, &mut environment) {
                    // Keep the preceding types when the true map is unreachable.
                    environment = previous;
                }
            }
        }
        Some((scope, environment))
    }

    // A nested comprehension can inherit a refinement only for the same binding.
    pub(in crate::infrastructure::mypy) fn comprehension_binding_identity(
        &self,
        key: &str,
    ) -> Option<(usize, TextSize)> {
        let name = key.split(['.', '[']).next()?;
        let binding = self.resolved_binding(name)?;
        self.stack
            .iter()
            .position(|scope| {
                scope
                    .bindings
                    .get(name)
                    .is_some_and(|candidate| std::ptr::eq(candidate, binding))
            })
            .map(|owner| (owner, binding.first_definition))
    }

    pub(in crate::infrastructure::mypy) fn enter_comprehension(
        &mut self,
        generators: &[Comprehension],
    ) {
        let owner = self
            .current()
            .comprehension_owner
            .unwrap_or(self.stack.len() - 1);
        self.enter(Kind::Function, &[], BTreeMap::new(), self.variables());
        self.stack.last_mut().unwrap().comprehension_owner = Some(owner);
        for generator in generators {
            self.declare_comprehension_target(&generator.target);
        }
        self.declare_filter_walruses(generators);
    }

    pub(in crate::infrastructure::mypy) fn bind_assignment_expression(
        &mut self,
        target: &Expr,
        ty: Option<Type>,
    ) {
        let (Some(owner), Expr::Name(name)) = (self.current().comprehension_owner, target) else {
            self.bind(target, ty);
            if let Expr::Name(name) = target {
                // Availability follows the reached store, not its textual offset or type.
                self.stack
                    .last_mut()
                    .unwrap()
                    .walrus_assigned
                    .insert(name.id.to_string(), true);
            }
            return;
        };
        let index = self.stack[owner]
            .redirects
            .get(name.id.as_str())
            .copied()
            .unwrap_or(owner);
        self.stack
            .last_mut()
            .unwrap()
            .redirects
            .insert(name.id.to_string(), index);
        self.clear_annotation_form(name.id.as_str());
        if index == owner {
            if let Some(binding) = self.stack[index].bindings.get_mut(name.id.as_str()) {
                if binding.infer_from_assignment && name.start() < binding.first_definition {
                    binding.first_definition = name.start();
                    binding.ty = None;
                }
            }
            self.bind(target, ty);
        }
        self.stack
            .last_mut()
            .unwrap()
            .walrus_assigned
            .insert(name.id.to_string(), true);
    }

    fn declare_comprehension_target(&mut self, target: &Expr) {
        match target {
            Expr::Name(_) => self.bind(target, None),
            Expr::Tuple(tuple) => {
                for target in &tuple.elts {
                    self.declare_comprehension_target(target);
                }
            }
            Expr::List(list) => {
                for target in &list.elts {
                    self.declare_comprehension_target(target);
                }
            }
            Expr::Starred(starred) => self.declare_comprehension_target(&starred.value),
            _ => {}
        }
    }

    pub(in crate::infrastructure::mypy) fn clear_comprehension_binding(
        environment: &mut BTreeMap<String, Type>,
        name: &str,
    ) {
        environment.retain(|key, _| {
            key != name
                && !key
                    .strip_prefix(name)
                    .is_some_and(|tail| tail.starts_with('.'))
        });
    }

    pub(in crate::infrastructure::mypy) fn bind_comprehension_target(
        &mut self,
        target: &Expr,
        ty: Type,
    ) -> BTreeMap<String, Type> {
        let mut environment = BTreeMap::new();
        bind_target(&self.context, target, ty, &mut environment);
        self.bind_comprehension_bindings(&environment);
        environment
    }

    fn bind_comprehension_bindings(&mut self, environment: &BTreeMap<String, Type>) {
        for (name, &ty) in environment {
            self.stack.last_mut().unwrap().bindings.insert(
                name.clone(),
                Binding {
                    // The element precedes its target in source text but executes after binding.
                    ty: Some(ty),
                    first_definition: TextSize::new(0),
                    infer_from_assignment: true,
                    callable: None,
                    module: None,
                    explicit: true,
                    module_hidden: Some(false),
                    annotation_form: None,
                    annotation_module: None,
                },
            );
        }
    }
}

fn bind_target(
    context: &types::Context,
    target: &Expr,
    ty: Type,
    environment: &mut BTreeMap<String, Type>,
) {
    match target {
        Expr::Name(name) => {
            environment.insert(name.id.to_string(), ty);
        }
        Expr::Tuple(tuple) => bind_items(context, &tuple.elts, ty, environment),
        Expr::List(list) => bind_items(context, &list.elts, ty, environment),
        Expr::Starred(starred) => bind_target(context, &starred.value, ty, environment),
        _ => {}
    }
}

fn bind_items(
    context: &types::Context,
    items: &[Expr],
    ty: Type,
    environment: &mut BTreeMap<String, Type>,
) {
    let node = context.node(ty);
    for (index, item) in items.iter().enumerate() {
        let ty = match node.as_deref() {
            Some(types::Node::Tuple(types)) => types.get(index).copied().unwrap_or(Type::Any),
            _ => context.iterable_item(ty).unwrap_or(Type::Any),
        };
        bind_target(context, item, ty, environment);
    }
}
