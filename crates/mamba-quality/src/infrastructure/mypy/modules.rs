//! Module exports share the workspace type context and reach a dependency fixpoint.
use super::{
    scopes::{module_declarations::ModuleDeclarations, module_exports::ExportState, Scopes},
    signatures::Signature,
    types::{Context, Type},
};
use crate::infrastructure::project::{self, Import, ModuleId, ProjectIndex};
use mamba_language::application::quality_syntax::python_ast::{Expr, Stmt};
use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet, VecDeque},
    rc::Rc,
    sync::Arc,
};
#[cfg(test)]
mod binder_identity;
mod builtins;
mod dependencies;
pub(super) use dependencies::{approved_stub_notes, only_once_import_note, MissingImport};
mod export_state;
mod import_cycles;
mod native;
mod providers;

// One normal member binding owns the receiver, signature and receiver error.
// It is handed directly to the same source Call, never cached across visits.
pub(super) struct BoundCallable {
    pub(super) expression: usize,
    pub(super) receiver: Type,
    pub(super) owner: (usize, super::scopes::Kind, Option<usize>),
    pub(super) signature: Arc<Signature>,
    pub(super) error: Option<String>,
}

#[derive(Clone, PartialEq, Eq)]
pub(super) struct Export {
    pub ty: Option<Type>,
    pub callable: Option<Arc<Signature>>,
    pub module: Option<String>,
    pub annotation_module: bool,
    pub explicit: bool,
    pub module_hidden: Option<bool>,
    pub alias: Option<Type>,
}

impl Export {
    // Locked providers without hidden metadata keep their existing visibility result.
    pub(super) fn is_hidden(&self) -> bool {
        self.module_hidden.unwrap_or(!self.explicit)
    }

    fn resolved(&self) -> bool {
        self.ty.is_some_and(|ty| !ty.is_any())
            || self.callable.is_some()
            || self.module.is_some()
            || self.alias.is_some_and(|ty| !ty.is_any())
    }
}

pub(super) type Exports = BTreeMap<String, Export>;
type ImportIssues = Vec<(ruff_text_size::TextRange, String)>;

pub(super) enum ImportDiagnostic {
    Issue(String),
    Missing(String, MissingImport),
}

pub(super) struct Modules {
    context: Rc<Context>,
    names: BTreeMap<(usize, String), ModuleId>,
    info: Vec<(usize, String, bool)>,
    exports: RefCell<Vec<Exports>>,
    export_states: RefCell<Vec<Option<Arc<ExportState>>>>,
    export_gaps: RefCell<Vec<BTreeSet<String>>>,
    export_uses: RefCell<Vec<BTreeSet<ModuleId>>>,
    namespaces: BTreeSet<(usize, String)>,
    shadow_namespaces: BTreeSet<(usize, String)>,
    embedded_sources: Vec<bool>,
    early_issues: RefCell<Vec<Option<ImportIssues>>>,
    reexports: Vec<BTreeMap<String, (String, String)>>,
    dependency_roots: Vec<Vec<std::path::PathBuf>>,
    untyped: BTreeSet<(usize, String)>,
    imported: BTreeSet<(usize, String)>,
}

#[derive(Clone)]
pub(super) struct Lookup {
    modules: Rc<Modules>,
    pub current: ModuleId,
}

impl Modules {
    pub(super) fn build(
        index: &ProjectIndex<'_>,
        context: Rc<Context>,
        workspace: &crate::domain::source::QualityWorkspace,
        untyped: BTreeSet<(usize, String)>,
        imported: BTreeSet<(usize, String)>,
        installed_sources: BTreeSet<ModuleId>,
    ) -> Rc<Self> {
        let (declarations, mut scopes) =
            ModuleDeclarations::prepare(index, &context, workspace, &installed_sources);
        // Embedded declarations add graph parents, but only non-embedded sources
        // can shadow the native child declarations through those parents.
        let mut shadow_namespaces = BTreeSet::new();
        for (module, source) in index.modules.iter().zip(&workspace.sources) {
            if super::dependencies::embedded::is_source(source) {
                continue;
            }
            shadow_namespaces.insert((module.project, module.name.clone()));
            for (offset, _) in module.name.match_indices('.') {
                shadow_namespaces.insert((module.project, module.name[..offset].into()));
            }
        }
        let modules = Rc::new(Self {
            context,
            untyped,
            imported,
            dependency_roots: workspace
                .projects
                .iter()
                .map(|project| {
                    project
                        .dependencies
                        .iter()
                        .filter(|dependency| dependency.kind == "installed-package-data")
                        .filter_map(|dependency| dependency.path.clone())
                        .collect()
                })
                .collect(),
            names: index.names.clone(),
            namespaces: index.namespaces.clone(),
            shadow_namespaces,
            embedded_sources: workspace
                .sources
                .iter()
                .map(super::dependencies::embedded::is_source)
                .collect(),
            early_issues: RefCell::new(vec![None; index.modules.len()]),
            export_states: RefCell::new(
                scopes
                    .iter()
                    .map(|scope| scope.as_ref().map(Scopes::module_export_state))
                    .collect(),
            ),
            export_gaps: RefCell::new(vec![BTreeSet::new(); index.modules.len()]),
            export_uses: RefCell::new(vec![BTreeSet::new(); index.modules.len()]),
            reexports: import_cycles::reexports(index, &declarations),
            info: index
                .modules
                .iter()
                .map(|module| (module.project, module.name.clone(), module.is_package))
                .collect(),
            exports: RefCell::new(
                scopes
                    .iter()
                    .map(|scope| scope.as_ref().map(Scopes::exports).unwrap_or_default())
                    .collect(),
            ),
        });
        for (id, source) in workspace.sources.iter().enumerate() {
            modules.export_gaps.borrow_mut()[id] =
                export_state::messages(source, modules.export_states.borrow()[id].as_deref());
        }
        let mut pending: VecDeque<_> = index.order.iter().copied().collect();
        let mut queued = vec![true; scopes.len()];
        while let Some(id) = pending.pop_front() {
            queued[id] = false;
            let Some(scope) = &mut scopes[id] else {
                continue;
            };
            let body = declarations[id].as_ref().unwrap().body();
            let module = &index.modules[id];
            let document = module
                .document
                .as_ref()
                .expect("a prepared module scope has a source document");
            // Each refresh uses the same real source identity and selected AST.
            // Continue/end-of-iteration restores any prior owner through RAII.
            let _source_owner = modules.context.source_declarations(
                &format!("{}:{}", module.project, module.name),
                &workspace.sources[id].sha256,
                document.target,
                body,
                None,
            );
            if modules.early_issues.borrow()[id].is_none() {
                let lookup = Self::lookup(&modules, id);
                let issues = lookup
                    .imports(body)
                    .into_iter()
                    .filter_map(|import| {
                        lookup
                            .import_issue(&import)
                            .map(|message| (import.range, message))
                    })
                    .collect();
                modules.early_issues.borrow_mut()[id] = Some(issues);
            }
            scope.attach_modules(Self::lookup(&modules, id), body);
            if index.modules[id].name == "ast"
                && super::dependencies::embedded::is_source(&workspace.sources[id])
            {
                scope.complete_embedded_ast_constant();
            }
            let exports = scope.exports();
            let state = Some(scope.module_export_state());
            let members_changed = modules.export_states.borrow()[id]
                .as_ref()
                .zip(state.as_ref())
                .is_some_and(|(previous, current)| current.members_changed(previous));
            if modules.exports.borrow()[id] == exports
                && modules.export_states.borrow()[id] == state
            {
                continue;
            }
            modules.export_gaps.borrow_mut()[id] =
                export_state::messages(&workspace.sources[id], state.as_deref());
            modules.exports.borrow_mut()[id] = exports;
            modules.export_states.borrow_mut()[id] = state;
            if members_changed {
                // A stable class object can pass through unchanged reexports
                // and callable returns. Walk their existing dependency edges.
                let mut reached = BTreeSet::from([id]);
                let mut dependents: VecDeque<_> = index.dependents[id].iter().copied().collect();
                while let Some(dependent) = dependents.pop_front() {
                    if !reached.insert(dependent) {
                        continue;
                    }
                    if !queued[dependent] {
                        queued[dependent] = true;
                        pending.push_back(dependent);
                    }
                    dependents.extend(index.dependents[dependent].iter().copied());
                }
            } else {
                for &dependent in &index.dependents[id] {
                    if !queued[dependent] {
                        queued[dependent] = true;
                        pending.push_back(dependent);
                    }
                }
            }
        }
        modules
    }

    pub(super) fn lookup(modules: &Rc<Self>, current: ModuleId) -> Lookup {
        Lookup {
            modules: modules.clone(),
            current,
        }
    }
}

impl Lookup {
    fn semantic_external_module_exists(&self, name: &str) -> bool {
        if name == "string.templatelib" {
            return self.modules.context.module_target(&self.namespace())
                == mamba_language::application::quality_syntax::PythonTarget::Py314;
        }
        let target = self.modules.context.module_target(&self.namespace());
        super::dependencies::embedded::available(name, target)
            || Self::native_pydantic_module(name)
            || matches!(name, "typing_extensions" | "pydantic.mypy")
    }

    pub(super) fn external_module_exists(&self, name: &str) -> bool {
        let semantic = self.semantic_external_module_exists(name);
        if semantic || name == "string.templatelib" {
            return semantic;
        }
        let relative = name.replace('.', "/");
        self.modules.dependency_roots[self.modules.info[self.current].0]
            .iter()
            .any(|root| {
                root.join(&relative).is_dir() || root.join(format!("{relative}.py")).is_file()
            })
    }

    pub(super) fn namespace(&self) -> String {
        let (project, name, _) = &self.modules.info[self.current];
        format!("{project}:{name}")
    }

    pub(super) fn is_package(&self) -> bool {
        self.modules.info[self.current].2
    }
    fn import_issue(&self, import: &Import) -> Option<String> {
        if let Some(message) = self.untyped_import_issue(&import.module) {
            return Some(message);
        }
        let member = import.member.as_ref()?;
        if member == "*"
            || (!self.module_exists(&import.module)
                && !matches!(import.module.as_str(), "typing" | "collections.abc"))
        {
            return None;
        }
        match self.named_import_member(&import.module, member) {
            Some(export) if export.is_hidden() => Some(format!(
                "Module \"{}\" does not explicitly export attribute \"{member}\"",
                import.module
            )),
            None => Some(format!(
                "Module \"{}\" has no attribute \"{member}\"{}",
                import.module,
                self.modules.context.standard_module_suggestion(
                    &import.module,
                    member,
                    self.modules.context.module_target(&self.namespace())
                )
            )),
            _ => None,
        }
    }

    fn checked_import_issue(&self, import: &Import) -> Option<String> {
        if import.member.is_none() {
            return self.import_issue(import);
        }
        let early = self.modules.early_issues.borrow()[self.current]
            .as_ref()
            .and_then(|issues| {
                issues
                    .iter()
                    .find(|(range, _)| *range == import.range)
                    .map(|(_, message)| message.clone())
            });
        let resolved = early.is_some()
            && import.member.as_ref().is_some_and(|member| {
                self.named_import_member(&import.module, member)
                    .is_some_and(|export| {
                        export.resolved() || !self.unresolved_import_cycle(&import.module, member)
                    })
            });
        early
            .filter(|_| !resolved)
            .or_else(|| self.import_issue(import))
    }

    pub(super) fn imports(&self, body: &[Stmt]) -> Vec<Import> {
        let (_, name, is_package) = &self.modules.info[self.current];
        project::imports(body, name, *is_package)
    }

    pub(super) fn module_exists(&self, module: &str) -> bool {
        let project = self.modules.info[self.current].0;
        self.modules.namespaces.contains(&(project, module.into())) || module == "copy"
    }

    fn module_id(&self, module: &str) -> Option<ModuleId> {
        self.modules
            .names
            .get(&(self.modules.info[self.current].0, module.into()))
            .copied()
    }
}

impl Scopes {
    pub(super) fn import_diagnostics(
        &self,
        body: &[Stmt],
    ) -> Vec<(ruff_text_size::TextRange, ImportDiagnostic)> {
        let Some(lookup) = &self.modules else {
            return Vec::new();
        };
        let mut seen = BTreeSet::new();
        let mut diagnostics = Vec::new();
        for import in lookup.imports(body) {
            if let Some(message) = lookup.checked_import_issue(&import) {
                diagnostics.push((import.range, ImportDiagnostic::Issue(message)));
            }
            let reason = lookup.approved_stub_issue(&import.module).or_else(|| {
                (!lookup.module_exists(&import.module)
                    && lookup.untyped_import_issue(&import.module).is_none()
                    && !lookup.external_module_exists(&import.module))
                .then_some(MissingImport::NotFound)
            });
            if let Some(reason) = reason {
                if seen.insert(import.module.clone()) {
                    diagnostics.push((
                        import.range,
                        ImportDiagnostic::Missing(import.module, reason),
                    ));
                }
            }
        }
        diagnostics
    }

    pub(super) fn module_expression(&self, expression: &Expr) -> Option<String> {
        let lookup = self.modules.as_ref()?;
        match expression {
            Expr::Name(name) => self.resolved_binding(name.id.as_str())?.module.clone(),
            Expr::Attribute(attribute) => {
                let parent = self.module_expression(&attribute.value)?;
                lookup.member(&parent, attribute.attr.as_str())?.module
            }
            _ => None,
        }
    }

    pub(super) fn module_member(&self, expression: &Expr) -> Option<Export> {
        let Expr::Attribute(attribute) = expression else {
            return None;
        };
        let module = self.module_expression(&attribute.value)?;
        self.modules
            .as_ref()?
            .member(&module, attribute.attr.as_str())
    }

    pub(super) fn callable_expression(&self, expression: &Expr) -> Option<Arc<Signature>> {
        self.callable_expression_in(expression, &self.environment())
    }

    pub(super) fn callable_expression_in(
        &self,
        expression: &Expr,
        environment: &BTreeMap<String, Type>,
    ) -> Option<Arc<Signature>> {
        self.callable_expression_in_with_binding(expression, environment)
            .map(|(signature, _)| signature)
    }

    pub(super) fn callable_expression_in_with_binding(
        &self,
        expression: &Expr,
        environment: &BTreeMap<String, Type>,
    ) -> Option<(Arc<Signature>, Option<BoundCallable>)> {
        if let Some(signature) = self.native_dataclass_function(expression) {
            return Some((signature, None));
        }
        if let Some(signature) = self.builtin_constructor_expression(expression) {
            return Some((signature, None));
        }
        let value_type = |expression: &Expr| {
            self.context
                .infer_with_calls(expression, environment, &|expression| {
                    self.expression_type(expression, environment)
                })
        };
        let callable = |ty| self.callable_value_signature(ty);
        match expression {
            Expr::Name(name) => self
                .callable(name.id.as_str())
                .cloned()
                .or_else(|| callable(value_type(expression)?))
                .map(|signature| (signature, None)),
            Expr::Attribute(attribute) => self
                .module_member(expression)
                .and_then(|export| export.callable)
                .map(|signature| (signature, None))
                .or_else(|| {
                    let receiver = value_type(&attribute.value)?;
                    let (signature, error) = self.context.class_method_call_binding_for(
                        receiver,
                        attribute.attr.as_str(),
                        self.context.module_target(self.loop_namespace()),
                    )?;
                    let binding = BoundCallable {
                        expression: expression as *const Expr as usize,
                        receiver,
                        owner: self.try_input_scope(),
                        signature: signature.clone(),
                        error,
                    };
                    Some((signature, Some(binding)))
                })
                .or_else(|| {
                    value_type(expression)
                        .and_then(callable)
                        .map(|signature| (signature, None))
                }),
            _ => value_type(expression)
                .and_then(callable)
                .map(|signature| (signature, None)),
        }
    }
}
