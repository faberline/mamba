//! Only proved module paths bypass an absent class declaration environment.
use super::*;
use crate::infrastructure::mypy::classes::{Method, MethodKind};
use mamba_language::application::quality_syntax::python_ast::{
    visitor::{walk_expr, walk_stmt, Visitor},
    Stmt,
};
use std::collections::BTreeSet;

impl Scopes {
    // The collector reuses one map for fields, methods, and nested classes.
    // Add builtin markers only when all consumers have this proved class view.
    pub(in crate::infrastructure::mypy::scopes) fn class_field_annotation_variables(
        &self,
        class: &StmtClassDef,
    ) -> types::Variables {
        let original = self.live_annotation_variables(&self.current().variables);
        if self.current().kind != Kind::Module
            || self.stack.len() != 1
            || class.type_params.is_some()
            || class.arguments.is_some()
        {
            return original;
        }
        let mut declarations = BTreeSet::new();
        let mut roots = MethodAnnotationRoots {
            context: &self.context,
            variables: &original,
            literal: &|value| self.canonical_literal_form(value, &original).is_some(),
            names: BTreeMap::new(),
            known: true,
            refusal: None,
        };
        for statement in &class.body {
            match statement {
                Stmt::AnnAssign(assignment) => {
                    let Expr::Name(name) = assignment.target.as_ref() else {
                        return original;
                    };
                    declarations.insert(name.id.to_string());
                    roots.visit_expr(&assignment.annotation);
                }
                Stmt::Pass(_) => {}
                Stmt::Expr(expression)
                    if matches!(expression.value.as_ref(), Expr::StringLiteral(_)) => {}
                // Method bodies have another annotation namespace. Nested
                // classes inherit this map; neither consumer is proved here.
                _ => return original,
            }
        }
        if !roots.known || declarations.contains("dict") || declarations.contains("list") {
            return original;
        }
        let Some(writes) = self.annotation_value_writes(&class.body) else {
            return original;
        };
        let mut imports = ClassImports::default();
        imports.visit_body(&class.body);
        if imports.unknown {
            return original;
        }
        let expanded = self.annotation_variables_in(&self.current().variables);
        let mut added = BTreeMap::new();
        for name in ["dict", "list"] {
            if roots.names.contains_key(name)
                && !original.contains_key(name)
                && self.unshadowed_builtin_annotation_root(name)
            {
                if let Some(&marker) = expanded.get(name) {
                    if self.context.builtin_constructor_alias(
                        name,
                        self.context.module_target(&self.namespace),
                    ) == Some(marker)
                    {
                        added.insert(name.to_string(), marker);
                    }
                }
            }
        }
        if added.is_empty()
            || !roots.names.iter().all(|(name, expression)| {
                if declarations.contains(name) || !clear_class_root(name, &writes, &imports) {
                    return false;
                }
                self.module_annotation_root_matches(name, expression, &original)
            })
        {
            return original;
        }
        let mut variables = original;
        for (name, marker) in added {
            variables.insert(name, marker);
        }
        variables
    }

    fn module_annotation_root_matches(
        &self,
        name: &str,
        expression: &Expr,
        variables: &types::Variables,
    ) -> bool {
        self.module_annotation_root_proof(name, expression, variables)
            .is_ok()
    }

    fn module_annotation_root_proof(
        &self,
        name: &str,
        expression: &Expr,
        variables: &types::Variables,
    ) -> Result<(), &'static str> {
        let Some(&ty) = variables.get(name) else {
            return self
                .unshadowed_builtin_annotation_root(name)
                .then_some(())
                .ok_or("annotation root has no input or unshadowed builtin receipt");
        };
        if matches!(
            self.context.node(ty).as_deref(),
            Some(types::Node::LiteralForm)
        ) {
            return (self.canonical_literal_form(expression, variables) == Some(ty))
                .then_some(())
                .ok_or("Literal input differs from its live canonical form");
        }
        // Prove markers at every type position, including parameters.
        // Every other name must agree with the actual module input.
        if self.context.is_callable_form(ty) {
            if self.canonical_callable_form(expression) != Some(ty) {
                return Err("Callable annotation root differs from its live canonical form");
            }
        } else if self.current().variables.get(name).copied() != Some(ty) {
            return Err("annotation root differs from the live module input");
        }
        // Do not inspect or guess the lexical dependencies of aliases,
        // applied generic types, or stored Callable signatures.
        (!matches!(ty, Type::Node(_))
            || matches!(
                self.context.node(ty).as_deref(),
                Some(
                    types::Node::Variable(_)
                        | types::Node::Class(_)
                        | types::Node::Instance(_)
                        | types::Node::TypingForm(_)
                )
            ))
        .then_some(())
        .ok_or("annotation root has an unproved alias, applied, or other node role")
    }

    // This footprint is local to one populate call. It stores no live lookup.
    pub(in crate::infrastructure::mypy::scopes) fn class_method_annotation_proof<'a>(
        &'a self,
        class: &'a StmtClassDef,
    ) -> impl Fn(&StmtFunctionDef, &types::Variables) -> Result<(), (u32, &'static str)> + 'a {
        let supported = (|| {
            let reject = |reason| Err((class.start().to_u32(), reason));
            if self.current().kind != Kind::Module {
                return reject("annotation proof owner is not a module");
            }
            if self.stack.len() != 1 {
                return reject("annotation proof has more than the module frame");
            }
            if self.modules.is_none() {
                return reject("annotation proof has no live module lookup");
            }
            if class.type_params.is_some() {
                return reject("class annotation proof has explicit type parameters");
            }
            let Some(receipt) = self.original_builtin_roots.as_ref() else {
                return reject("original module annotation-root footprint is absent");
            };
            if receipt.namespace != self.namespace {
                return reject("original annotation-root footprint belongs to another namespace");
            }
            if receipt.target != self.context.module_target(&self.namespace) {
                return reject("original annotation-root footprint belongs to another target");
            }
            Ok(())
        })();
        let footprint = std::cell::OnceCell::new();
        move |function, variables| {
            supported?;
            if function.type_params.is_some() {
                return Err((
                    function.start().to_u32(),
                    "method annotation has explicit type parameters",
                ));
            }
            let Some(returns) = function.returns.as_deref() else {
                return Err((function.start().to_u32(), "method has no return annotation"));
            };
            let mut roots = MethodAnnotationRoots {
                context: &self.context,
                variables,
                literal: &|value| self.canonical_literal_form(value, variables).is_some(),
                names: BTreeMap::new(),
                known: true,
                refusal: None,
            };
            for parameter in function.parameters.iter() {
                if let Some(annotation) = parameter.annotation() {
                    roots.visit_expr(annotation);
                }
            }
            roots.visit_expr(returns);
            if !roots.known {
                return Err(roots.refusal.unwrap_or((
                    returns.start().to_u32(),
                    "annotation type-position proof is incomplete",
                )));
            }
            if !roots.names.iter().any(|(name, _)| {
                variables
                    .get(name)
                    .is_some_and(|&ty| self.context.is_callable_form(ty))
            }) {
                return Err((
                    returns.start().to_u32(),
                    "method annotations have no input Callable marker",
                ));
            }
            let (writes, imports) = match footprint.get_or_init(|| {
                let writes = self
                    .annotation_value_writes(&class.body)
                    .ok_or("class annotation-write footprint is incomplete")?;
                let mut imports = ClassImports::default();
                imports.visit_body(&class.body);
                (!imports.unknown)
                    .then_some((writes, imports))
                    .ok_or("class import footprint is incomplete")
            }) {
                Ok(value) => value,
                Err(reason) => return Err((class.start().to_u32(), *reason)),
            };
            for (name, expression) in &roots.names {
                if let Err(reason) = clear_class_root_proof(name, writes, imports) {
                    return Err((expression.start().to_u32(), reason));
                }
                if let Err(reason) = self.module_annotation_root_proof(name, expression, variables)
                {
                    // Missing input alone proves nothing. Only this method path
                    // can use an original direct import and its live export.
                    if !variables.contains_key(name)
                        && self.imported_annotation_constructor_root(name, expression)
                    {
                        continue;
                    }
                    if self
                        .source_method_self_type(class, variables, name)
                        .is_none()
                    {
                        return Err((expression.start().to_u32(), reason));
                    }
                }
            }
            Ok(())
        }
    }

    // Called only by the original-document setter, never from selected bodies.
    pub(in crate::infrastructure::mypy::scopes) fn install_original_builtin_roots(
        &mut self,
        body: &[Stmt],
    ) {
        self.original_builtin_roots = None;
        if self.stack.len() != 1 || self.current().kind != Kind::Module {
            return;
        }
        let Some(writes) = self.annotation_value_writes(body) else {
            return;
        };
        let mut imports = ClassImports::default();
        imports.visit_body(body);
        if imports.unknown {
            return;
        }
        // Unlike the module-local write collector, this small hazard pass
        // visits nested bodies. Any explicit global root blocks that builtin.
        let mut globals = GlobalAnnotationRoots::default();
        globals.visit_body(body);
        let mut import_counts = ModuleAnnotationImportCounts::default();
        import_counts.visit_body(body);
        let mut constructor_imports = BTreeMap::new();
        for statement in body {
            if !matches!(statement, Stmt::ImportFrom(import) if import.level == 0) {
                continue;
            }
            for import in
                crate::infrastructure::project::imports(std::slice::from_ref(statement), "", false)
            {
                let Some(member) = import.member else {
                    continue;
                };
                let Some((constructor, _)) = types::Context::annotation_constructor(&member) else {
                    continue;
                };
                if !matches!(import.module.as_str(), "typing" | "typing_extensions")
                    || types::Context::annotation_constructor(&import.local).map(|(role, _)| role)
                        != Some(constructor)
                    || import_counts.0.get(&import.local) != Some(&1)
                    || writes.contains_key(&import.local)
                    || globals.names.contains(&import.local)
                {
                    continue;
                }
                constructor_imports.insert(
                    import.local,
                    OriginalAnnotationImport {
                        module: import.module,
                        member,
                        range: import.range,
                        explicit: import.explicit,
                        constructor,
                    },
                );
            }
        }
        let mut roots: BTreeSet<_> = writes.into_keys().collect();
        roots.extend(imports.names);
        self.original_builtin_roots = Some(Arc::new(OriginalBuiltinRoots {
            namespace: self.namespace.clone(),
            target: self.context.module_target(&self.namespace),
            roots,
            global_hazards: globals.names,
            constructor_imports,
        }));
    }

    fn imported_annotation_constructor_root(&self, name: &str, expression: &Expr) -> bool {
        let Expr::Name(root) = expression else {
            return false;
        };
        let scope = self.current();
        let Some(receipt) = &self.original_builtin_roots else {
            return false;
        };
        let Some(lookup) = &self.modules else {
            return false;
        };
        if !root.ctx.is_load()
            || root.id.as_str() != name
            || self.stack.len() != 1
            || scope.kind != Kind::Module
            || receipt.namespace != self.namespace
            || lookup.namespace() != self.namespace
            || receipt.target != self.context.module_target(&self.namespace)
            || receipt.global_hazards.contains(name)
            || scope.redirects.contains_key(name)
            || scope.first_unbound_local_assignments.contains_key(name)
            || scope.variables.contains_key(name)
            || !scope.imported_names.contains(name)
            || scope.imported_functions.contains_key(name)
        {
            return false;
        }
        let Some(import) = receipt.constructor_imports.get(name) else {
            return false;
        };
        // Source graph declarations, aliases and local replacements cannot
        // borrow the locked native typing module's constructor role.
        if lookup.module_exists(&import.module)
            || types::Context::annotation_constructor(&import.member).map(|(role, _)| role)
                != Some(import.constructor)
            || self.context.standard_collection_symbol(
                &import.module,
                &import.member,
                receipt.target,
            ) != Some(true)
        {
            return false;
        }
        let Some(binding) = scope.bindings.get(name) else {
            return false;
        };
        let Some(canonical) = lookup.standard_module_member(&import.module, &import.member) else {
            return false;
        };
        let Some(export) = lookup.named_import_member(&import.module, &import.member) else {
            return false;
        };
        let hidden = if import.explicit {
            Some(false)
        } else {
            lookup.named_import_hidden(&import.module, &import.member, &export)
        };
        export == canonical
            && export.alias.is_none()
            && !export.annotation_module
            && export.module.is_none()
            && binding.first_definition == import.range.start()
            && !binding.infer_from_assignment
            && binding.explicit == import.explicit
            && binding.module_hidden == hidden
            && binding.annotation_form.is_none()
            && binding.annotation_module.is_none()
            && binding.ty == export.ty
            && binding.callable == export.callable
            && binding.module == export.module
    }

    fn unshadowed_builtin_annotation_root(&self, name: &str) -> bool {
        let scope = self.current();
        let Some(receipt) = &self.original_builtin_roots else {
            return false;
        };
        if self.stack.len() != 1
            || scope.kind != Kind::Module
            || receipt.namespace != self.namespace
            || receipt.target != self.context.module_target(&self.namespace)
            || !self
                .modules
                .as_ref()
                .is_some_and(|lookup| lookup.namespace() == self.namespace)
            || receipt.roots.contains(name)
            || receipt.global_hazards.contains(name)
            || scope.bindings.contains_key(name)
            || scope.variables.contains_key(name)
            || scope.imported_names.contains(name)
            || scope.imported_functions.contains_key(name)
            || scope.redirects.contains_key(name)
            || scope.first_unbound_local_assignments.contains_key(name)
        {
            return false;
        }
        if types::Context::builtin_name(name).is_some() {
            return true;
        }
        self.context
            .builtin_constructor_alias(name, receipt.target)
            .is_some_and(|ty| {
                matches!(self.context.node(ty).as_deref(),
                    Some(types::Node::TypingForm(form)) if form.starts_with("builtin:"))
            })
    }

    pub(super) fn decorated_instance_method(
        &self,
        class: &StmtClassDef,
        function: &StmtFunctionDef,
        method: &Method,
    ) -> Option<Arc<Signature>> {
        // The module owns this refresh. Class-local decorators need their own
        // declaration environment; a method body environment is not that view.
        if self.current().kind != Kind::Module
            || class.type_params.is_some()
            || method.kind != MethodKind::Instance
            || method.setter.is_some()
            || method.signature.constructor
            || !method.signature.overloads.is_empty()
            || matches!(function.name.as_str(), "__init__" | "__new__")
            || function.decorator_list.is_empty()
            || super::super::super::signatures::no_type_check(function)
            || self.is_overload(function)
        {
            return None;
        }
        let writes = self.annotation_value_writes(&class.body)?;
        if writes.get(function.name.as_str()) != Some(&Some(function.name.start())) {
            return None;
        }
        let selected = crate::infrastructure::mypy::classes::statements::collect(
            &class.body,
            self.context.module_target(&self.namespace),
        );
        if !selected.iter().any(|statement| {
            matches!(statement, Stmt::FunctionDef(selected) if std::ptr::eq(selected, function))
        }) {
            return None;
        }
        let mut imports = ClassImports::default();
        imports.visit_body(&class.body);
        if imports.unknown {
            return None;
        }
        let lookup = self.modules.as_ref()?;
        for decorator in &function.decorator_list {
            // Factory calls, bare aliases, and descriptor combinations are not
            // admitted by this module-member-only environment proof.
            if !matches!(decorator.expression, Expr::Attribute(_)) {
                return None;
            }
            let path = types::Context::annotation_form_name(&decorator.expression)?;
            let mut path = path.split('.');
            let root = path.next()?;
            // Mypy's class-body lookup supplies these two implicit strings
            // before it considers module globals (semanal.py:6708).
            if !clear_class_root(root, &writes, &imports) {
                return None;
            }
            let binding = self.resolved_binding(root)?;
            let mut module = self.live_annotation_module(binding)?;
            let mut members = path.peekable();
            while let Some(member) = members.next() {
                let export = lookup.member(&module, member)?;
                if export.is_hidden() {
                    return None;
                }
                if members.peek().is_some() {
                    if !export.annotation_module {
                        return None;
                    }
                    module = export.module?;
                } else if export.module.is_some()
                    || !export.callable.is_some_and(|signature| {
                        !signature.implicit && !signature.ignored && !signature.constructor
                    })
                {
                    return None;
                }
            }
        }
        let applied = self.typed_decorator_application(
            function,
            method.signature.clone(),
            self.environment(),
        );
        let result = applied.result?;
        let node = self.context.node(result)?;
        let types::Node::Callable(signature) = node.as_ref() else {
            // An arbitrary descriptor result is not an ordinary method.
            return None;
        };
        Some(Arc::new(Signature {
            name: method.signature.name.clone(),
            owner: method.signature.owner.clone(),
            plain_source_implicit_name: false,
            ..signature.as_ref().clone()
        }))
    }
}

fn clear_class_root(
    root: &str,
    writes: &BTreeMap<String, Option<TextSize>>,
    imports: &ClassImports,
) -> bool {
    clear_class_root_proof(root, writes, imports).is_ok()
}

fn clear_class_root_proof(
    root: &str,
    writes: &BTreeMap<String, Option<TextSize>>,
    imports: &ClassImports,
) -> Result<(), &'static str> {
    if matches!(root, "__qualname__" | "__module__") {
        return Err("annotation root is an implicit class-local name");
    }
    if writes.contains_key(root) {
        return Err("annotation root has a class-local write");
    }
    if imports.names.contains(root) {
        return Err("annotation root has a class-local import");
    }
    Ok(())
}

#[derive(Default)]
struct GlobalAnnotationRoots {
    names: BTreeSet<String>,
}

impl<'ast> Visitor<'ast> for GlobalAnnotationRoots {
    fn visit_stmt(&mut self, statement: &'ast Stmt) {
        if let Stmt::Global(global) = statement {
            self.names
                .extend(global.names.iter().map(ToString::to_string));
        }
        walk_stmt(self, statement);
    }
}

// A refusal-only lexical check, not an annotation parser. The existing AST
// visitor walks only explicit type positions. Opaque syntax has no proof here.
struct MethodAnnotationRoots<'a, 'ast> {
    context: &'a types::Context,
    variables: &'a types::Variables,
    literal: &'a dyn Fn(&Expr) -> bool,
    names: BTreeMap<String, &'ast Expr>,
    known: bool,
    refusal: Option<(u32, &'static str)>,
}

impl<'ast> Visitor<'ast> for MethodAnnotationRoots<'_, 'ast> {
    fn visit_expr(&mut self, expression: &'ast Expr) {
        if !self.known {
            return;
        }
        match expression {
            Expr::Name(name) if name.ctx.is_load() => {
                self.names.entry(name.id.to_string()).or_insert(expression);
            }
            Expr::Subscript(subscript) => {
                self.visit_expr(&subscript.value);
                if (self.literal)(&subscript.value) {
                    return;
                }
                if self
                    .context
                    .typing_form(&subscript.value, self.variables)
                    .as_deref()
                    == Some("Annotated")
                {
                    // Match the existing binder/type-analysis role: metadata
                    // does not bind type variables (typeanal.py:2790).
                    let Expr::Tuple(arguments) = subscript.slice.as_ref() else {
                        self.known = false;
                        self.refusal.get_or_insert((
                            expression.start().to_u32(),
                            "Annotated input has no tuple payload",
                        ));
                        return;
                    };
                    if arguments.elts.len() < 2 {
                        self.known = false;
                        self.refusal.get_or_insert((
                            expression.start().to_u32(),
                            "Annotated input has fewer than two payloads",
                        ));
                        return;
                    }
                    self.visit_expr(&arguments.elts[0]);
                } else {
                    self.visit_expr(&subscript.slice);
                }
            }
            Expr::List(_) | Expr::Tuple(_) => walk_expr(self, expression),
            Expr::BinOp(binary)
                if binary.op
                    == mamba_language::application::quality_syntax::python_ast::Operator::BitOr =>
            {
                self.visit_expr(&binary.left);
                self.visit_expr(&binary.right);
            }
            Expr::NoneLiteral(_) | Expr::EllipsisLiteral(_) => {}
            // This also refuses forward strings and qualified payloads. No
            // spelling or parse of a string can prove class lexical ownership.
            _ => {
                self.known = false;
                self.refusal.get_or_insert((
                    expression.start().to_u32(),
                    "annotation expression has no proved type-position role",
                ));
            }
        }
    }
}

#[derive(Default)]
struct ModuleAnnotationImportCounts(BTreeMap<String, usize>);

impl<'ast> Visitor<'ast> for ModuleAnnotationImportCounts {
    fn visit_stmt(&mut self, statement: &'ast Stmt) {
        if matches!(statement, Stmt::FunctionDef(_) | Stmt::ClassDef(_)) {
            return;
        }
        for import in
            crate::infrastructure::project::imports(std::slice::from_ref(statement), "", false)
        {
            *self.0.entry(import.local).or_default() += 1;
        }
        walk_stmt(self, statement);
    }

    fn visit_expr(&mut self, _expression: &'ast Expr) {}
}

#[derive(Default)]
struct ClassImports {
    names: BTreeSet<String>,
    unknown: bool,
}

impl<'a> Visitor<'a> for ClassImports {
    fn visit_stmt(&mut self, statement: &'a Stmt) {
        match statement {
            // Their binders/default expressions are handled by the existing
            // annotation_value_writes collector. Their bodies have other owners.
            Stmt::FunctionDef(_) | Stmt::ClassDef(_) => {}
            Stmt::Import(import) => {
                for alias in &import.names {
                    let name = alias.asname.as_ref().map_or_else(
                        || alias.name.as_str().split('.').next().unwrap(),
                        |name| name.as_str(),
                    );
                    self.names.insert(name.into());
                }
            }
            Stmt::ImportFrom(import) => {
                for alias in &import.names {
                    if alias.name.as_str() == "*" {
                        self.unknown = true;
                    } else {
                        self.names
                            .insert(alias.asname.as_ref().unwrap_or(&alias.name).to_string());
                    }
                }
            }
            Stmt::Global(_) | Stmt::Nonlocal(_) => self.unknown = true,
            _ => walk_stmt(self, statement),
        }
    }

    fn visit_expr(&mut self, expression: &'a Expr) {
        match expression {
            Expr::Attribute(attribute) if !attribute.ctx.is_load() => self.unknown = true,
            Expr::Subscript(subscript) if !subscript.ctx.is_load() => self.unknown = true,
            // Nested expression scopes cannot import names into this class.
            Expr::Lambda(_) => {}
            _ => walk_expr(self, expression),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{ports::ProjectStore, source::PythonVersion};
    use crate::infrastructure::{
        filesystem::Filesystem, mypy::modules::Modules, project::ProjectIndex,
    };
    use std::{collections::BTreeSet, fs, rc::Rc};

    #[derive(Debug)]
    struct MethodRootOutcome {
        root: bool,
        proof: Result<(), (u32, &'static str)>,
        producer_gaps: Vec<String>,
        fresh_returned_u: bool,
        local_fresh_returned_u: bool,
        local_enclosing_gap: bool,
        input_type: Option<types::Type>,
        exact_tuple_t: bool,
    }

    fn method_root_proof(source: &str, shadow_typing: bool) -> MethodRootOutcome {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("probe.py");
        fs::write(&path, source).unwrap();
        let mut paths = vec![path];
        if shadow_typing {
            let typing = root.path().join("typing.py");
            fs::write(&typing, "Tuple = tuple\nCallable = object\n").unwrap();
            paths.push(typing);
        }
        let mut workspace = Filesystem.load(&paths, true);
        workspace.projects[0].python_version = PythonVersion::Py312;
        let index = ProjectIndex::new(&workspace);
        let context = Rc::new(types::Context::default());
        let modules = Modules::build(
            &index,
            context.clone(),
            &workspace,
            BTreeSet::new(),
            BTreeSet::new(),
            BTreeSet::new(),
        );
        let id = index
            .modules
            .iter()
            .position(|module| module.name == "probe")
            .unwrap();
        let document = index.modules[id].document.as_ref().unwrap();
        let body = &document.parsed.syntax().body;
        let lookup = Modules::lookup(&modules, id);
        let namespace = lookup.namespace();
        let _source_owner = context.borrow_returned_callable_source(
            &namespace,
            &workspace.sources[id].sha256,
            document.target,
            body,
        );
        let _declarations = context.source_declarations(
            &namespace,
            &workspace.sources[id].sha256,
            document.target,
            body,
            None,
        );
        let mut scopes = Scopes::new(body, context, namespace);
        scopes.set_terminal_none_source(&workspace.sources[id].text);
        scopes.set_module_export_plan(body);
        scopes.attach_modules(lookup, body);
        let producer_gaps = scopes.returned_callable_gaps();
        let class = body
            .iter()
            .find_map(|statement| match statement {
                Stmt::ClassDef(class) => Some(class),
                _ => None,
            })
            .unwrap();
        let method = class
            .body
            .iter()
            .find_map(|statement| match statement {
                Stmt::FunctionDef(function) => Some(function),
                _ => None,
            })
            .unwrap();
        let probe_type = *scopes.current().variables.get("Probe").unwrap();
        let probe_id = scopes.context.class_id(probe_type).unwrap();
        let produced_class = scopes.context.class(probe_id);
        assert_eq!(produced_class.namespace, scopes.namespace);
        assert_eq!(produced_class.offset, class.name.start().to_u32());
        let produced_method = produced_class.methods.get("get").unwrap();
        assert!(matches!(produced_method.kind, MethodKind::Instance));
        assert_eq!(produced_method.signature.owner.as_deref(), Some("Probe"));
        let input_type = produced_method
            .signature
            .parameters
            .iter()
            .find(|parameter| parameter.name == "value")
            .and_then(|parameter| parameter.ty);
        let outer_t = scopes.current().variables.get("T").copied();
        let exact_tuple_t = outer_t.is_some_and(|outer_t| {
            input_type.is_some_and(|input| {
                matches!(scopes.context.node(input).as_deref(), Some(types::Node::Tuple(items)) if items.as_slice() == [outer_t])
            })
        });
        let fresh_returned_u = produced_method
            .signature
            .returns
            .and_then(|ty| scopes.context.callable_signature(ty))
            .is_some_and(|returned| {
                let [bound_u] = returned.generic.as_ref() else {
                    return false;
                };
                matches!(
                    scopes.context.node(*bound_u).as_deref(),
                    Some(types::Node::Variable(variable)) if variable.name == "U"
                ) && returned.parameters.len() == 1
                    && returned.parameters[0].ty == Some(*bound_u)
                    && returned.returns == Some(*bound_u)
                    && produced_method.signature.variables.get("U").copied() != Some(*bound_u)
            });
        let annotation = method
            .parameters
            .iter()
            .find_map(|parameter| parameter.annotation())
            .unwrap();
        let Expr::Subscript(subscript) = annotation else {
            panic!("test method parameter is subscripted")
        };
        let Expr::Name(name) = subscript.value.as_ref() else {
            panic!("test method parameter has a name root")
        };
        let root = scopes.imported_annotation_constructor_root(name.id.as_str(), &subscript.value);
        let variables = scopes.live_annotation_variables(&scopes.current().variables);
        let proof_result = scopes.class_method_annotation_proof(class)(method, &variables);
        let class_variables = scopes
            .context
            .class_scope_variables(probe_id, scopes.variables());
        scopes.enter_proved_class(
            class,
            &workspace.sources[id].sha256,
            document.target,
            BTreeMap::new(),
            class_variables,
            &produced_class.generic,
        );
        let local = scopes.function_declaration_signature(method).unwrap();
        let original_u = scopes.stack[0].variables.get("U").copied();
        let local_fresh_returned_u = local
            .returns
            .and_then(|ty| scopes.context.callable_signature(ty))
            .is_some_and(|returned| {
                let [bound_u] = returned.generic.as_ref() else {
                    return false;
                };
                original_u.is_some_and(|original| *bound_u != original)
                    && returned.parameters.len() == 1
                    && returned.parameters[0].ty == Some(*bound_u)
                    && returned.returns == Some(*bound_u)
            });
        let local_enclosing_gap = scopes.returned_callable_gaps().iter().any(|gap| {
            gap.contains(&format!(
                "complete enclosing-bound receipt at byte {}",
                method.start().to_u32()
            ))
        });
        MethodRootOutcome {
            root,
            proof: proof_result,
            producer_gaps,
            fresh_returned_u,
            local_fresh_returned_u,
            local_enclosing_gap,
            input_type,
            exact_tuple_t,
        }
    }

    #[test]
    fn direct_typing_method_root_requires_original_live_import() {
        let method = "T = TypeVar('T')\nU = TypeVar('U')\nclass Probe:\n    def get(self, value: Tuple[T]) -> Callable[[U], U]: ...\n";
        let direct = method_root_proof(
            &format!("from typing import Tuple, Callable, TypeVar\n{method}"),
            false,
        );
        assert!(
            direct.root && direct.proof.is_ok(),
            "direct import: {direct:?}"
        );
        assert!(direct.fresh_returned_u, "direct producer: {direct:?}");
        assert!(
            direct.local_fresh_returned_u && !direct.local_enclosing_gap,
            "direct local producer: {direct:?}"
        );
        assert!(direct.exact_tuple_t, "direct Tuple[T] input: {direct:?}");
        assert!(
            direct.input_type.is_some_and(|ty| ty != types::Type::Any),
            "direct input type: {direct:?}"
        );
        assert!(
            direct
                .producer_gaps
                .iter()
                .all(|gap| !gap.contains("method producer receipt refused")),
            "direct producer: {direct:?}"
        );
        for (source, root_expected) in [
            (format!("from typing import Tuple as Alias, Callable, TypeVar\n{}", method.replace("Tuple", "Alias")), false),
            (format!("from typing import Tuple, Callable, TypeVar\nfrom typing import Tuple\n{method}"), false),
            (format!("from typing import Tuple, Callable, TypeVar\nTuple = None\n{method}"), false),
            ("from typing import Tuple, Callable, TypeVar\nT = TypeVar('T')\nU = TypeVar('U')\nclass Probe:\n    Tuple = tuple\n    def get(self, value: Tuple[T]) -> Callable[[U], U]: ...\n".into(), true),
        ] {
            let result = method_root_proof(&source, false);
            assert_eq!(result.root, root_expected, "import receipt: {source}");
            assert!(result.proof.is_err(), "unexpected proof: {source}");
            assert!(
                !result.fresh_returned_u,
                "unproved root produced a fresh binder: {source}: {result:?}"
            );
            assert!(
                !result.local_fresh_returned_u,
                "unproved root produced a local fresh binder: {source}: {result:?}"
            );
            assert!(
                result
                    .producer_gaps
                    .iter()
                    .any(|gap| gap.contains("method producer receipt refused")),
                "missing producer refusal: {source}: {result:?}"
            );
        }
        let shadow = method_root_proof(
            &format!("from typing import Tuple, Callable, TypeVar\n{method}"),
            true,
        );
        assert!(
            !shadow.root && shadow.proof.is_err() && !shadow.fresh_returned_u,
            "source graph shadow: {shadow:?}"
        );
        assert!(
            !shadow.local_fresh_returned_u,
            "source graph shadow gained a local binder: {shadow:?}"
        );
    }

    #[test]
    fn direct_typing_method_root_keeps_public_callable_receipt() {
        let source = "from typing import Tuple, Callable, TypeVar\nT = TypeVar('T')\nU = TypeVar('U')\nclass Probe:\n    def get(self, value: Tuple[T]) -> Callable[[U], U]: ...\ndef use(probe: Probe) -> None:\n    reveal_type(probe.get((1,)))\n    probe.get(1)\n";
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("probe.py");
        fs::write(&path, source).unwrap();
        let mut workspace = Filesystem.load(std::slice::from_ref(&path), true);
        workspace.projects[0].python_version = PythonVersion::Py312;
        let observer =
            crate::infrastructure::mypy::test_observation::arm(&path, &workspace.sources[0].sha256);
        let report = crate::infrastructure::mypy::check(&workspace);
        let receipt = observer
            .take()
            .expect("public checker did not observe the source")
            .expect("public checker did not retain the original Probe method");
        assert!(
            receipt.source_identity,
            "public class source identity: {receipt:?}"
        );
        assert!(receipt.exact_tuple_t, "public Tuple[T] input: {receipt:?}");
        assert!(
            receipt.fresh_returned_u,
            "public returned U binder: {receipt:?}"
        );
        assert_eq!(report.checked_files, 1);
        let ordered_rules: Vec<_> = report
            .diagnostics
            .iter()
            .map(|diagnostic| (diagnostic.location.line, diagnostic.rule.as_str()))
            .collect();
        assert_eq!(
            ordered_rules,
            [(5, "empty-body"), (7, "misc"), (8, "arg-type")],
            "public diagnostic order: {report:?}"
        );
        let reveal = report
            .diagnostics
            .iter()
            .find(|diagnostic| diagnostic.location.line == 7 && diagnostic.rule == "misc")
            .expect("valid tuple call did not reveal its returned callable");
        let expected_return = receipt
            .rendered_returned_callable
            .as_deref()
            .expect("proved returned Callable has no rendered type");
        assert!(
            !expected_return.contains("Any") && !expected_return.contains("Never"),
            "proved returned Callable rendered as an erased type: {receipt:?}"
        );
        assert_eq!(
            reveal.message,
            format!("Revealed type is \"{expected_return}\""),
            "public call did not consume the proved U-to-U return: {report:?}"
        );
        let argument_errors: Vec<_> = report
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.rule == "arg-type")
            .map(|diagnostic| (diagnostic.location.line, diagnostic.message.as_str()))
            .collect();
        assert_eq!(
            argument_errors.len(),
            1,
            "public argument count: {report:?}"
        );
        assert!(
            argument_errors[0].0 == 8
                && argument_errors[0].1.contains("incompatible type \"int\"")
                && argument_errors[0].1.contains("expected \"tuple["),
            "public tuple argument rejection: {report:?}"
        );
        assert!(
            report.remaining.iter().all(|gap| {
                !gap.contains("stable legacy producer identity")
                    && !gap.contains("method producer receipt refused")
                    && !gap.contains("Returned Callable binder lacks a complete enclosing-bound receipt at byte 95")
            }),
            "public source owner: {report:?}"
        );
    }

    #[test]
    fn class_scope_returned_callable_requires_its_live_original_body() {
        let source = "from typing import Tuple, Callable, TypeVar\nT = TypeVar('T')\nU = TypeVar('U')\nclass Probe:\n    def get(self, value: Tuple[T]) -> Callable[[U], U]: ...\nclass Other:\n    def get(self, value: Tuple[T]) -> Callable[[U], U]: ...\n";
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("probe.py");
        fs::write(&path, source).unwrap();
        let mut workspace = Filesystem.load(std::slice::from_ref(&path), true);
        workspace.projects[0].python_version = PythonVersion::Py312;
        let index = ProjectIndex::new(&workspace);
        let context = Rc::new(types::Context::default());
        let modules = Modules::build(
            &index,
            context.clone(),
            &workspace,
            BTreeSet::new(),
            BTreeSet::new(),
            BTreeSet::new(),
        );
        let id = index
            .modules
            .iter()
            .position(|module| module.name == "probe")
            .unwrap();
        let document = index.modules[id].document.as_ref().unwrap();
        let body = &document.parsed.syntax().body;
        let lookup = Modules::lookup(&modules, id);
        let namespace = lookup.namespace();
        let source_owner = context.borrow_returned_callable_source(
            &namespace,
            &workspace.sources[id].sha256,
            document.target,
            body,
        );
        let declarations = context.source_declarations(
            &namespace,
            &workspace.sources[id].sha256,
            document.target,
            body,
            None,
        );
        let mut scopes = Scopes::new(body, context.clone(), namespace);
        scopes.set_terminal_none_source(&workspace.sources[id].text);
        scopes.set_module_export_plan(body);
        scopes.attach_modules(lookup, body);
        let classes: Vec<_> = body
            .iter()
            .filter_map(|statement| match statement {
                Stmt::ClassDef(class) => Some(class),
                _ => None,
            })
            .collect();
        let [probe, other] = classes.as_slice() else {
            panic!("expected two original class ASTs")
        };
        fn method(
            class: &mamba_language::application::quality_syntax::python_ast::StmtClassDef,
        ) -> &StmtFunctionDef {
            class
                .body
                .iter()
                .find_map(|statement| match statement {
                    Stmt::FunctionDef(function) => Some(function),
                    _ => None,
                })
                .unwrap()
        }
        let probe_id = scopes
            .context
            .class_id(*scopes.current().variables.get(probe.name.as_str()).unwrap())
            .unwrap();
        let generic = scopes.context.class(probe_id).generic.clone();
        let variables = scopes
            .context
            .class_scope_variables(probe_id, scopes.variables());
        scopes.enter_proved_class(
            probe,
            &workspace.sources[id].sha256,
            document.target,
            BTreeMap::new(),
            variables,
            &generic,
        );
        let local = scopes
            .function_declaration_signature(method(probe))
            .unwrap();
        let original_u = *scopes.stack[0].variables.get("U").unwrap();
        let fresh = |signature: &Signature, context: &types::Context| {
            signature
                .returns
                .and_then(|ty| context.callable_signature(ty))
                .is_some_and(|returned| {
                    returned.generic.len() == 1
                        && returned.generic[0] != original_u
                        && returned.parameters.len() == 1
                        && returned.parameters[0].ty == Some(returned.generic[0])
                        && returned.returns == Some(returned.generic[0])
                })
        };
        assert!(
            fresh(&local, &context),
            "original class-body method has no fresh U: {local:?}"
        );
        assert!(
            !scopes
                .returned_callable_gaps()
                .iter()
                .any(|gap| gap.contains(&format!(
                    "complete enclosing-bound receipt at byte {}",
                    method(probe).start().to_u32()
                ))),
            "original class-body receipt left a gap: {:?}",
            scopes.returned_callable_gaps()
        );

        // Other.get is from the same parsed source, but the active lexical
        // frame belongs to Probe. Its live source lease alone is insufficient.
        let foreign = Signature::from_function(
            method(other),
            &scopes.context,
            &scopes.live_annotation_variables(&scopes.current().variables),
            &scopes.function_binder_input(method(other), &[]),
        );
        assert!(
            !fresh(&foreign, &context),
            "foreign body gained a fresh U: {foreign:?}"
        );
        assert!(
            scopes
                .returned_callable_gaps()
                .iter()
                .any(|gap| gap.contains(&format!(
                    "complete enclosing-bound receipt at byte {}",
                    method(other).start().to_u32()
                ))),
            "foreign body lost its refusal: {:?}",
            scopes.returned_callable_gaps()
        );

        // A changed lexical bound set is not the class entry's receipt.
        scopes.stack[0].bound_variables.insert(types::Type::Any);
        let incomplete = scopes.refresh_function_annotations(method(probe), &local);
        assert!(
            !fresh(&incomplete, &context),
            "incomplete bound set gained a fresh U: {incomplete:?}"
        );
        assert!(
            scopes
                .returned_callable_gaps()
                .iter()
                .any(|gap| gap.contains(&format!(
                    "complete enclosing-bound receipt at byte {}",
                    method(probe).start().to_u32()
                ))),
            "incomplete bound set lost its refusal: {:?}",
            scopes.returned_callable_gaps()
        );
        scopes.stack[0].bound_variables.remove(&types::Type::Any);
        let restored = scopes.refresh_function_annotations(method(probe), &local);
        assert!(
            fresh(&restored, &context),
            "restored bound set did not bind U: {restored:?}"
        );
        assert!(
            !scopes
                .returned_callable_gaps()
                .iter()
                .any(|gap| gap.contains(&format!(
                    "complete enclosing-bound receipt at byte {}",
                    method(probe).start().to_u32()
                ))),
            "restored bound set left a scope gap: {:?}",
            scopes.returned_callable_gaps()
        );

        // A nested owner can register the same AST under a different SHA
        // after the class receipt was issued. The receipt must then refuse.
        let different_sha256 = "0".repeat(64);
        assert_ne!(different_sha256, workspace.sources[id].sha256);
        let nested_declarations = context.source_declarations(
            &scopes.namespace,
            &different_sha256,
            document.target,
            body,
            None,
        );
        let return_expression = method(probe).returns.as_deref().unwrap();
        assert_eq!(
            context.source_expression_identity(return_expression),
            Some((
                scopes.namespace.clone(),
                different_sha256,
                format!("{:?}", document.target),
            ))
        );
        let replaced = scopes.refresh_function_annotations(method(probe), &local);
        assert!(
            !fresh(&replaced, &context),
            "replaced source owner gained a fresh U: {replaced:?}"
        );
        assert!(
            scopes
                .returned_callable_gaps()
                .iter()
                .any(|gap| gap.contains(&format!(
                    "complete enclosing-bound receipt at byte {}",
                    method(probe).start().to_u32()
                ))),
            "replaced source owner lost its refusal: {:?}",
            scopes.returned_callable_gaps()
        );
        drop(nested_declarations);
        assert_eq!(
            context.source_expression_identity(return_expression),
            Some((
                scopes.namespace.clone(),
                workspace.sources[id].sha256.clone(),
                format!("{:?}", document.target),
            ))
        );
        let restored_owner = scopes.refresh_function_annotations(method(probe), &local);
        assert!(
            fresh(&restored_owner, &context),
            "restored source owner did not bind U: {restored_owner:?}"
        );
        assert!(
            !scopes
                .returned_callable_gaps()
                .iter()
                .any(|gap| gap.contains(&format!(
                    "complete enclosing-bound receipt at byte {}",
                    method(probe).start().to_u32()
                ))),
            "restored source owner left a scope gap: {:?}",
            scopes.returned_callable_gaps()
        );

        // The same AST cannot be admitted under a different source hash.
        scopes.leave();
        let variables = scopes
            .context
            .class_scope_variables(probe_id, scopes.variables());
        scopes.enter_proved_class(
            probe,
            &"0".repeat(64),
            document.target,
            BTreeMap::new(),
            variables,
            &generic,
        );
        let wrong_source = scopes
            .function_declaration_signature(method(probe))
            .unwrap();
        assert!(
            !fresh(&wrong_source, &context),
            "wrong source hash gained a fresh U: {wrong_source:?}"
        );
        assert!(
            scopes
                .returned_callable_gaps()
                .iter()
                .any(|gap| gap.contains(&format!(
                    "complete enclosing-bound receipt at byte {}",
                    method(probe).start().to_u32()
                ))),
            "wrong source hash lost its refusal: {:?}",
            scopes.returned_callable_gaps()
        );
        scopes.leave();
        let variables = scopes
            .context
            .class_scope_variables(probe_id, scopes.variables());
        scopes.enter_proved_class(
            probe,
            &workspace.sources[id].sha256,
            document.target,
            BTreeMap::new(),
            variables,
            &generic,
        );
        let local = scopes
            .function_declaration_signature(method(probe))
            .unwrap();
        assert!(
            fresh(&local, &context),
            "restored original source did not bind U: {local:?}"
        );
        assert!(
            !scopes
                .returned_callable_gaps()
                .iter()
                .any(|gap| gap.contains(&format!(
                    "complete enclosing-bound receipt at byte {}",
                    method(probe).start().to_u32()
                ))),
            "restored original source left a scope gap: {:?}",
            scopes.returned_callable_gaps()
        );

        // Releasing the source declaration lease invalidates the receipt even
        // while the returned-Callable source lease remains live.
        drop(declarations);
        let stale = scopes.refresh_function_annotations(method(probe), &local);
        assert!(
            !fresh(&stale, &context),
            "stale body gained a fresh U: {stale:?}"
        );
        assert!(
            scopes
                .returned_callable_gaps()
                .iter()
                .any(|gap| gap.contains(&format!(
                    "complete enclosing-bound receipt at byte {}",
                    method(probe).start().to_u32()
                ))),
            "stale source owner lost its refusal: {:?}",
            scopes.returned_callable_gaps()
        );
        drop(source_owner);
    }

    #[test]
    fn returned_callable_validation_keeps_unbound_u_outside_proved_method() {
        let source = "from typing import Tuple, Callable, TypeVar\nT = TypeVar('T')\nU = TypeVar('U')\nclass Probe:\n    def get(self, value: Tuple[T]) -> Callable[[U], U]: ...\n    sibling: U\noutside: U\n";
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("probe.py");
        fs::write(&path, source).unwrap();
        let mut workspace = Filesystem.load(std::slice::from_ref(&path), true);
        workspace.projects[0].python_version = PythonVersion::Py312;
        let report = crate::infrastructure::mypy::check(&workspace);
        assert_eq!(report.checked_files, 1);
        assert!(
            report.diagnostics.iter().all(|diagnostic| {
                !(diagnostic.location.line == 5 && diagnostic.rule == "valid-type")
            }),
            "proved inner U was rejected: {report:?}"
        );
        assert!(
            report.diagnostics.iter().any(|diagnostic| {
                diagnostic.location.line == 6
                    && diagnostic.rule == "valid-type"
                    && diagnostic.message.contains("U")
                    && diagnostic.message.contains("unbound")
            }),
            "class sibling U was bound by the method receipt: {report:?}"
        );
        assert!(
            report.diagnostics.iter().any(|diagnostic| {
                diagnostic.location.line == 7
                    && diagnostic.rule == "valid-type"
                    && diagnostic.message.contains("U")
                    && diagnostic.message.contains("unbound")
            }),
            "module outside U was bound by the method receipt: {report:?}"
        );
    }

    #[test]
    fn returned_callable_validation_requires_live_same_ast_source_owner() {
        let source = "from typing import Tuple, Callable, TypeVar\nT = TypeVar('T')\nU = TypeVar('U')\nclass Probe:\n    def get(self, value: Tuple[T]) -> Callable[[U], U]: ...\noutside: U\n";
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("probe.py");
        fs::write(&path, source).unwrap();
        let mut workspace = Filesystem.load(std::slice::from_ref(&path), true);
        workspace.projects[0].python_version = PythonVersion::Py312;
        let index = ProjectIndex::new(&workspace);
        let context = Rc::new(types::Context::default());
        let modules = Modules::build(
            &index,
            context.clone(),
            &workspace,
            BTreeSet::new(),
            BTreeSet::new(),
            BTreeSet::new(),
        );
        let id = index
            .modules
            .iter()
            .position(|module| module.name == "probe")
            .unwrap();
        let document = index.modules[id].document.as_ref().unwrap();
        let body = &document.parsed.syntax().body;
        let lookup = Modules::lookup(&modules, id);
        let namespace = lookup.namespace();
        let _source_owner = context.borrow_returned_callable_source(
            &namespace,
            &workspace.sources[id].sha256,
            document.target,
            body,
        );
        let _declarations = context.source_declarations(
            &namespace,
            &workspace.sources[id].sha256,
            document.target,
            body,
            None,
        );
        let mut scopes = Scopes::new(body, context.clone(), namespace);
        scopes.set_terminal_none_source(&workspace.sources[id].text);
        scopes.set_module_export_plan(body);
        scopes.attach_modules(lookup, body);
        let class = body
            .iter()
            .find_map(|statement| match statement {
                Stmt::ClassDef(class) => Some(class),
                _ => None,
            })
            .unwrap();
        let method = class
            .body
            .iter()
            .find_map(|statement| match statement {
                Stmt::FunctionDef(function) => Some(function),
                _ => None,
            })
            .unwrap();
        let outside = body
            .iter()
            .find_map(|statement| match statement {
                Stmt::AnnAssign(assignment) => Some(assignment.annotation.as_ref()),
                _ => None,
            })
            .unwrap();
        let probe_type = *scopes.current().variables.get("Probe").unwrap();
        let probe_id = context.class_id(probe_type).unwrap();
        let generic = context.class(probe_id).generic.clone();
        let variables = context.class_scope_variables(probe_id, scopes.variables());
        scopes.enter_proved_class(
            class,
            &workspace.sources[id].sha256,
            document.target,
            BTreeMap::new(),
            variables,
            &generic,
        );
        let local = scopes.function_declaration_signature(method).unwrap();
        let refreshed = scopes.refresh_function_annotations(method, &local);
        let annotation_variables = scopes.annotation_variables_in(&refreshed.variables);
        let original_u = *scopes.stack[0].variables.get("U").unwrap();
        let return_expression = method.returns.as_deref().unwrap();
        let Expr::Subscript(callable) = return_expression else {
            panic!("method return is not Callable")
        };
        let Expr::Tuple(items) = callable.slice.as_ref() else {
            panic!("method return lacks parameter and result")
        };
        let [Expr::List(parameters), returned_u] = items.elts.as_slice() else {
            panic!("method return lacks ordinary parameters")
        };
        let [parameter_u] = parameters.elts.as_slice() else {
            panic!("method return lacks one U parameter")
        };
        let validation = context
            .returned_callable_validation(
                method,
                refreshed.returns,
                &annotation_variables,
                &scopes.namespace,
                &workspace.sources[id].sha256,
                document.target,
            )
            .expect("real returned Callable producer has no validation receipt");
        assert_eq!(
            context.returned_callable_validation_name(parameter_u),
            Some(original_u)
        );
        assert_eq!(
            context.returned_callable_validation_name(returned_u),
            Some(original_u)
        );
        assert_eq!(context.returned_callable_validation_name(outside), None);

        let different_sha256 = "0".repeat(64);
        assert_ne!(different_sha256, workspace.sources[id].sha256);
        let replacement = context.source_declarations(
            &scopes.namespace,
            &different_sha256,
            document.target,
            body,
            None,
        );
        assert_eq!(
            context.source_expression_identity(return_expression),
            Some((
                scopes.namespace.clone(),
                different_sha256,
                format!("{:?}", document.target),
            ))
        );
        assert_eq!(context.returned_callable_validation_name(parameter_u), None);
        assert_eq!(context.returned_callable_validation_name(returned_u), None);
        assert!(
            context
                .returned_callable_validation(
                    method,
                    refreshed.returns,
                    &annotation_variables,
                    &scopes.namespace,
                    &workspace.sources[id].sha256,
                    document.target,
                )
                .is_none(),
            "stale completed binder was accepted under a different live SHA"
        );
        drop(replacement);
        assert_eq!(
            context.returned_callable_validation_name(parameter_u),
            Some(original_u)
        );
        drop(validation);
        assert_eq!(context.returned_callable_validation_name(parameter_u), None);
        let restored = context
            .returned_callable_validation(
                method,
                refreshed.returns,
                &annotation_variables,
                &scopes.namespace,
                &workspace.sources[id].sha256,
                document.target,
            )
            .expect("original live source did not restore the completed binder");
        assert_eq!(
            context.returned_callable_validation_name(parameter_u),
            Some(original_u)
        );
        drop(restored);
        assert_eq!(context.returned_callable_validation_name(parameter_u), None);
    }
}
