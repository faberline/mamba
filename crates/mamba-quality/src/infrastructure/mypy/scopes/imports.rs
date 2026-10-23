//! Imports bind into the existing lexical scope without changing local declarations.
use super::super::modules::{Export, Exports, Lookup};
use super::*;
use crate::infrastructure::project;

impl Scopes {
    pub(crate) fn attach_modules(&mut self, lookup: Lookup, body: &[Stmt]) {
        self.modules = Some(lookup);
        self.install_module_globals();
        self.bind_imports(body);
        self.refresh_type_aliases(body);
        self.class_namespace_imports(body);
        let mut variables = self.variables();
        self.context.refresh_legacy_variables(body, &mut variables);
        self.stack.last_mut().unwrap().variables = variables;
        self.refresh_classes(body);
        self.refresh_imported_annotations(body);
        self.refresh_type_aliases(body);
        self.refresh_classes(body);
        self.infer_declarations(body);
        if self.refresh_function_decorators(body) {
            self.infer_declarations(body);
        }
        self.class_alias_annotations(body);
    }

    fn refresh_imported_annotations(&mut self, body: &[Stmt]) {
        for statement in body {
            let Stmt::AnnAssign(assignment) = statement else {
                continue;
            };
            if self.redefined_module_global(&assignment.target) {
                continue;
            }
            if self.alias_definition(statement) {
                continue;
            }
            let Expr::Name(name) = assignment.target.as_ref() else {
                continue;
            };
            let ty = self.annotation(&assignment.annotation);
            if let Some(binding) = self.binding_mut(name.id.as_str()) {
                if binding.first_definition == name.start() {
                    binding.ty = ty;
                    binding.annotation_form = None;
                    binding.annotation_module = None;
                }
            }
        }
    }

    pub(crate) fn exports(&self) -> Exports {
        let mut exports: Exports = self
            .current()
            .bindings
            .iter()
            .map(|(name, binding)| {
                (
                    name.clone(),
                    Export {
                        annotation_module: self.export_annotation_module(name, binding),
                        ty: binding.ty,
                        callable: binding.callable.clone(),
                        module: binding.module.clone(),
                        explicit: binding.explicit,
                        module_hidden: binding.module_hidden,
                        alias: self.export_annotation_alias(name, binding),
                    },
                )
            })
            .collect();
        self.finalize_module_exports(&mut exports);
        exports
    }

    pub(crate) fn bind_imports(&mut self, body: &[Stmt]) {
        self.bind_imports_in(body, false);
    }

    pub(crate) fn bind_checked_imports(&mut self, statement: &Stmt) -> bool {
        self.bind_imports_in(std::slice::from_ref(statement), true)
    }

    fn bind_imports_in(&mut self, body: &[Stmt], checked: bool) -> bool {
        let Some(lookup) = self.modules.clone() else {
            return false;
        };
        let owner = self.try_input_scope();
        let mut no_event = checked;
        let mut visited = false;
        self.refresh_module_export_state();
        let annotation_writes = if checked {
            None
        } else {
            self.annotation_value_writes(body)
        };
        for import in lookup.imports(body) {
            visited = true;
            if import.member.as_deref() == Some("*") {
                no_event = false;
                if let Some(imported_exports) = lookup.exports(&import.module) {
                    let has_all = lookup.source_has_all(&import.module).unwrap_or(false);
                    for (name, export) in imported_exports {
                        if export.explicit && (!name.starts_with('_') || has_all) {
                            let explicit = true;
                            let _ = self.insert_import(
                                name.clone(),
                                export,
                                import.range.start(),
                                explicit,
                                Some(false),
                                annotation_forms::ImportForm {
                                    origin: Some((import.module.clone(), name.clone())),
                                    live: checked
                                        || annotation_writes
                                            .as_ref()
                                            .is_some_and(|writes| !writes.contains_key(&name)),
                                    checked,
                                },
                            );
                        }
                    }
                }
                continue;
            }
            let fresh = checked && self.fresh_import_start(&import);
            let mut resolved = false;
            let export = if let Some(member) = &import.member {
                let selected = lookup.named_import_member(&import.module, member);
                resolved = selected.is_some();
                selected.unwrap_or(Export {
                    annotation_module: false,
                    ty: Some(Type::Any),
                    callable: None,
                    module: None,
                    explicit: false,
                    module_hidden: None,
                    alias: None,
                })
            } else {
                let module = if import.alias.is_none() {
                    import.local.clone()
                } else {
                    import.module.clone()
                };
                let resolved = (lookup.module_exists(&module)
                    || lookup.standard_module_exists(&module)
                    || lookup.typed_child_module_exists(&module)
                    || Lookup::native_pydantic_module(&module)
                    || module == "functools"
                    || (module == "string" && import.module == "string.templatelib")
                    || matches!(
                        module.as_str(),
                        "builtins"
                            | "typing"
                            | "collections"
                            | "collections.abc"
                            | "sys"
                            | "string.templatelib"
                    ))
                .then_some(module);
                Export {
                    annotation_module: resolved
                        .as_deref()
                        .is_some_and(|module| lookup.annotation_module_exists(module)),
                    ty: resolved.is_none().then_some(Type::Any),
                    callable: None,
                    module: resolved,
                    explicit: false,
                    module_hidden: None,
                    alias: None,
                }
            };
            // Only an actual named export with a material value is qualified.
            // Dynamic/missing exports and ordinary module fallbacks stay unknown.
            let fresh = fresh
                && resolved
                && export
                    .ty
                    .is_some_and(|ty| !ty.is_any() && ty != Type::Never);
            no_event &= fresh;
            // Mypy's dummy import assignment keeps the first source function.
            let admitted = checked && self.admitted_import(&import);
            if admitted && self.retains_imported_function(&import.local, &export) {
                no_event = false;
                continue;
            }
            let origin = admitted
                .then(|| self.imported_function_origin(&import, &export))
                .flatten();
            let expected_origin = fresh.then(|| origin.clone()).flatten();
            let local = import.local.clone();
            self.stack
                .last_mut()
                .unwrap()
                .imported_functions
                .remove(&local);
            let explicit = import.explicit;
            let module_hidden = if explicit {
                Some(false)
            } else if let Some(member) = &import.member {
                lookup.named_import_hidden(&import.module, member, &export)
            } else {
                Some(true)
            };
            let annotation_live = checked
                || annotation_writes
                    .as_ref()
                    .is_some_and(|writes| !writes.contains_key(&import.local));
            let annotation_origin = import
                .member
                .as_ref()
                .map(|member| (import.module.clone(), member.clone()));
            let alias = export.alias;
            let inserted = self.insert_import(
                import.local,
                export,
                import.range.start(),
                explicit,
                module_hidden,
                annotation_forms::ImportForm {
                    origin: annotation_origin,
                    live: annotation_live,
                    checked,
                },
            );
            if let Some(origin) = origin {
                self.stack
                    .last_mut()
                    .unwrap()
                    .imported_functions
                    .insert(local.clone(), origin);
            }
            no_event &= inserted.is_some_and(|binding| {
                // Source freshness and the actual resolver selected this value.
                // Equality here checks every live declaration field, not identity.
                self.current().bindings.get(&local) == Some(&binding)
                    && self.current().variables.get(&local).copied() == alias
                    && self.current().imported_functions.get(&local) == expected_origin.as_ref()
            });
        }
        visited && no_event && self.try_input_scope() == owner
    }

    fn fresh_import_start(&self, import: &project::Import) -> bool {
        let scope = self.current();
        scope.kind == Kind::Function
            && scope.comprehension_owner.is_none()
            && !scope.redirects.contains_key(&import.local)
            && !scope.bindings.contains_key(&import.local)
            && !scope.variables.contains_key(&import.local)
            && !scope.imported_names.contains(&import.local)
            && !scope.imported_functions.contains_key(&import.local)
            && !scope.redefinitions.contains(&import.local)
    }

    fn insert_import(
        &mut self,
        name: String,
        mut export: Export,
        first_definition: TextSize,
        explicit: bool,
        module_hidden: Option<bool>,
        annotation: annotation_forms::ImportForm,
    ) -> Option<Binding> {
        let capture = annotation.checked;
        let index = self.binding_scope(&name);
        if export.callable.is_none() {
            if let Some(&form) = self.stack[index].variables.get(&name) {
                if let Some(node) = self.context.node(form) {
                    if let types::Node::TypingForm(form) = node.as_ref() {
                        if let Some(signature) = self.context.dataclass_function(form) {
                            export.ty = Some(self.context.callable_type(signature.clone()));
                            export.callable = Some(signature);
                        }
                    } else if self.context.is_class_value(form)
                        && self
                            .context
                            .class_id(form)
                            .is_some_and(|id| self.context.class(id).namespace == "dataclasses")
                    {
                        export.ty = Some(form);
                    }
                }
            }
        }
        let annotation_live = annotation.live
            && self.annotation_import_refresh_live(
                &name,
                first_definition,
                &export,
                annotation.origin.as_ref(),
                annotation.checked,
            );
        let annotation_module = annotation_live
            .then(|| self.import_annotation_module(&export, annotation.origin.as_ref()))
            .flatten();
        let annotation_form = annotation_live
            .then(|| self.import_callable_form(export.alias, annotation.origin))
            .flatten();
        if export.alias.is_none()
            && self.stack[index]
                .variables
                .get(&name)
                .is_some_and(|&ty| self.context.is_callable_form(ty))
        {
            self.stack[index].variables.remove(&name);
        }
        if let Some(alias) = export.alias {
            self.stack[index].variables.insert(name.clone(), alias);
        }
        let binding = Binding {
            ty: export.ty,
            callable: export.callable,
            module: export.module,
            infer_from_assignment: false,
            first_definition,
            explicit,
            module_hidden,
            annotation_form,
            annotation_module,
        };
        let proof = capture.then(|| binding.clone());
        self.stack[index].bindings.insert(name, binding);
        proof
    }
}

pub(super) fn import_redefinitions(body: &[Stmt]) -> std::collections::BTreeSet<String> {
    let mut imported = std::collections::BTreeSet::new();
    let mut redefinitions = std::collections::BTreeSet::new();
    for statement in body {
        for import in project::imports(std::slice::from_ref(statement), "", false) {
            imported.insert(import.local);
        }
        let name = match statement {
            Stmt::FunctionDef(function) => Some(&function.name),
            Stmt::ClassDef(class) => Some(&class.name),
            _ => None,
        };
        if let Some(name) = name.filter(|name| imported.contains(name.as_str())) {
            redefinitions.insert(name.to_string());
        }
    }
    redefinitions
}

impl Scopes {
    pub(crate) fn import_redefinition(&self, name: &str) -> bool {
        self.current().redefinitions.contains(name)
    }
}

#[derive(Clone, PartialEq, Eq)]
pub(super) struct ImportedFunction {
    range: ruff_text_size::TextRange,
    ty: Type,
    module: String,
    member: String,
}

impl Scopes {
    fn plain_imported_function(&self, ty: Option<Type>) -> Option<Type> {
        let ty = ty?;
        let node = self.context.node(ty)?;
        let types::Node::Callable(signature) = node.as_ref() else {
            return None;
        };
        // Missing annotations on a wholly untyped function mean Any. A missing
        // resolved annotation on a typed declaration is not evidence of Any.
        let known = |ty: Option<Type>| match ty {
            None => signature.implicit,
            Some(ty @ Type::Node(_)) => self.context.node(ty).is_some_and(|node| {
                let types::Node::Instance(id) = node.as_ref() else {
                    return false;
                };
                let class = self.context.class(*id);
                class.generic.is_empty()
                    && !class.protocol
                    && self.context.class_mro(*id).is_ok_and(|lineage| {
                        lineage.into_iter().all(|owner| {
                            let class = self.context.class(owner);
                            class.any_base_origins.is_empty()
                                && !class.base_types.iter().any(|ty| ty.is_any())
                        })
                    })
            }),
            Some(Type::Never) => false,
            Some(_) => true,
        };
        (signature.name.is_some()
            && signature.definition_module.is_some()
            && signature.owner.is_none()
            && signature.bound_receiver.is_none()
            && signature.native_symbol.is_none()
            && !signature.constructor
            && !signature.ignored
            && !signature.ellipsis
            && signature.generic.is_empty()
            && signature.overloads.is_empty()
            && signature.narrowing.is_none()
            && known(signature.returns)
            && signature.parameters.iter().all(|parameter| {
                matches!(
                    parameter.kind,
                    super::super::signatures::Kind::PositionalOnly
                        | super::super::signatures::Kind::PositionalOrKeyword
                ) && known(parameter.ty)
            }))
        .then_some(ty)
    }

    fn tracked_imported_function(&self, name: &str) -> Option<&ImportedFunction> {
        let scope = self.current();
        if scope.kind != Kind::Function || scope.redirects.contains_key(name) {
            return None;
        }
        let origin = scope.imported_functions.get(name)?;
        let binding = scope.bindings.get(name)?;
        (binding.first_definition == origin.range.start()
            && binding.ty == Some(origin.ty)
            && binding.module.is_none()
            && self.plain_imported_function(Some(origin.ty)).is_some())
        .then_some(origin)
    }

    fn retains_imported_function(&self, name: &str, export: &Export) -> bool {
        self.tracked_imported_function(name).is_some()
            && self.plain_imported_function(export.ty).is_some()
    }

    fn imported_function_origin(
        &self,
        import: &project::Import,
        export: &Export,
    ) -> Option<ImportedFunction> {
        let scope = self.current();
        if scope.kind != Kind::Function
            || scope.redirects.contains_key(&import.local)
            || !self.can_start_imported_function(import)
        {
            return None;
        }
        let member = import
            .member
            .as_ref()
            .filter(|member| member.as_str() != "*")?;
        let ty = self.plain_imported_function(export.ty)?;
        Some(ImportedFunction {
            range: import.range,
            ty,
            module: import.module.clone(),
            member: member.clone(),
        })
    }

    pub(in crate::infrastructure::mypy) fn import_assignments(
        &mut self,
        body: &[Stmt],
    ) -> Vec<(String, Type, Type)> {
        self.prepare_import_assignments(body);
        let Some(lookup) = &self.modules else {
            return Vec::new();
        };
        lookup
            .imports(body)
            .into_iter()
            .filter_map(|import| {
                let member = import
                    .member
                    .as_ref()
                    .filter(|member| member.as_str() != "*")?;
                let origin = self.tracked_imported_function(&import.local)?;
                if origin.module == import.module && origin.member == *member {
                    return None;
                }
                let export = lookup.named_import_member(&import.module, member)?;
                let imported = self.plain_imported_function(export.ty)?;
                Some((import.local, origin.ty, imported))
            })
            .collect()
    }
}

impl Scopes {
    fn admitted_import(&self, import: &project::Import) -> bool {
        self.current()
            .import_admission
            .as_ref()
            .is_some_and(|admission| {
                admission.ranges.get(&import.range.start()) == Some(&import.range.end())
            })
    }

    fn can_start_imported_function(&self, import: &project::Import) -> bool {
        if !self.admitted_import(import) {
            return false;
        }
        let scope = self.current();
        if scope
            .import_admission
            .as_ref()
            .and_then(|admission| admission.first.get(&import.local))
            != Some(&import.range)
        {
            return false;
        }
        let Some(binding) = scope.bindings.get(&import.local) else {
            return true;
        };
        // A same/later prebound import may be replaced by this first checked
        // import. An earlier untracked binding is not a proven first symbol.
        binding.first_definition >= import.range.start()
            && scope
                .import_admission
                .as_ref()
                .is_some_and(|admission| admission.ranges.contains_key(&binding.first_definition))
            && !binding.infer_from_assignment
            && binding.module.is_none()
            && self.plain_imported_function(binding.ty).is_some()
    }

    fn prepare_import_assignments(&mut self, body: &[Stmt]) {
        if self.current().kind != Kind::Function || self.current().import_admission.is_none() {
            return;
        }
        let Some(lookup) = &self.modules else {
            return;
        };
        let supported = lookup.imports(body).into_iter().all(|import| {
            if !self.admitted_import(&import) {
                return false;
            }
            let Some(member) = import.member.as_deref().filter(|member| *member != "*") else {
                return true;
            };
            let tracked = self.tracked_imported_function(&import.local).is_some();
            let known = lookup
                .named_import_member(&import.module, member)
                .and_then(|export| self.plain_imported_function(export.ty))
                .is_some();
            if tracked {
                known
            } else {
                !known || self.can_start_imported_function(&import)
            }
        });
        if !supported {
            let scope = self.stack.last_mut().unwrap();
            scope.import_admission = None;
            scope.imported_functions.clear();
        }
    }
}

// This is source-range selection, not a replacement import resolver. Only
// statements the checker actually visits may create a collision origin.
#[derive(Clone, PartialEq, Eq)]
pub(super) struct ImportAdmission {
    ranges: BTreeMap<TextSize, TextSize>,
    first: BTreeMap<String, ruff_text_size::TextRange>,
}

pub(super) fn import_admission(
    body: &[Stmt],
    target: mamba_language::application::quality_syntax::PythonTarget,
) -> Option<ImportAdmission> {
    let mut ranges = BTreeMap::new();
    let mut first = BTreeMap::new();
    for statement in super::super::classes::statements::collect(body, target) {
        match statement {
            Stmt::Import(_) | Stmt::ImportFrom(_) => {
                let mut names = std::collections::BTreeSet::new();
                for import in project::imports(std::slice::from_ref(statement), "", false) {
                    if !names.insert(import.local.clone()) {
                        return None;
                    }
                    ranges.insert(import.range.start(), import.range.end());
                    first.entry(import.local).or_insert(import.range);
                }
            }
            Stmt::For(_) | Stmt::While(_) | Stmt::Try(_) | Stmt::With(_) | Stmt::Match(_)
                if contains_lexical_import(statement) =>
            {
                // Their semantic import ordering is not established by the
                // existing if selector. Keep the whole scope on the old path.
                return None;
            }
            _ => {}
        }
    }
    Some(ImportAdmission { ranges, first })
}

fn contains_lexical_import(statement: &Stmt) -> bool {
    use mamba_language::application::quality_syntax::python_ast::visitor::{walk_stmt, Visitor};
    struct Imports(bool);
    impl<'a> Visitor<'a> for Imports {
        fn visit_stmt(&mut self, statement: &'a Stmt) {
            match statement {
                Stmt::Import(_) | Stmt::ImportFrom(_) => self.0 = true,
                Stmt::FunctionDef(_) | Stmt::ClassDef(_) => {}
                _ => walk_stmt(self, statement),
            }
        }
    }
    let mut imports = Imports(false);
    imports.visit_stmt(statement);
    imports.0
}
