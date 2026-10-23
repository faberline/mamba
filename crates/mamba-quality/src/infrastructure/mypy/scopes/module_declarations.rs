//! One selected source view feeds all module declaration passes.
use super::super::{classes::statements, types::Context};
use super::Scopes;
use crate::{
    domain::source::QualityWorkspace,
    infrastructure::project::{ModuleId, ProjectIndex},
};
use mamba_language::application::quality_syntax::{
    python_ast::{ExceptHandler, Stmt},
    PythonTarget,
};
use std::{borrow::Cow, collections::BTreeSet, rc::Rc};

pub(in crate::infrastructure::mypy) struct ModuleDeclarations<'a>(
    Vec<super::super::types::ReturnedCallableSourceGuard>,
    Cow<'a, [Stmt]>,
);

impl<'a> ModuleDeclarations<'a> {
    pub(in crate::infrastructure::mypy) fn prepare(
        index: &'a ProjectIndex<'_>,
        context: &Rc<Context>,
        workspace: &QualityWorkspace,
        installed_sources: &BTreeSet<ModuleId>,
    ) -> (Vec<Option<Self>>, Vec<Option<Scopes>>) {
        let mut declarations: Vec<_> = index
            .modules
            .iter()
            .enumerate()
            .map(|(id, module)| {
                module.document.as_ref().map(|document| {
                    if installed_sources.contains(&id) {
                        return Self::installed(&document.parsed.syntax().body, document.target);
                    }
                    let select_embedded_declarations =
                        super::super::dependencies::embedded::selects_declarations(
                            &module.name,
                            &workspace.sources[id],
                        );
                    ModuleDeclarations::new(
                        &document.parsed.syntax().body,
                        document.target,
                        select_embedded_declarations,
                    )
                })
            })
            .collect();
        for (id, module) in index.modules.iter().enumerate() {
            context.set_module_stub(
                format!("{}:{}", module.project, module.name),
                workspace.sources[id].is_stub,
            );
        }
        for (id, module) in index.modules.iter().enumerate() {
            if let (Some(document), Some(declarations)) = (&module.document, &mut declarations[id])
            {
                let namespace = format!("{}:{}", module.project, module.name);
                let original = context.register_returned_callable_source(
                    &namespace,
                    &workspace.sources[id].sha256,
                    document.target,
                    &document.parsed.syntax().body,
                );
                let selected = context.register_returned_callable_source(
                    &namespace,
                    &workspace.sources[id].sha256,
                    document.target,
                    declarations.body(),
                );
                declarations.0.extend([original, selected]);
            }
        }
        let scopes: Vec<_> = index
            .modules
            .iter()
            .zip(&declarations)
            .enumerate()
            .map(|(id, (module, declarations))| {
                module.document.as_ref().map(|document| {
                    let namespace = format!("{}:{}", module.project, module.name);
                    context.set_module_target(namespace.clone(), document.target);
                    let body = declarations.as_ref().unwrap().body();
                    // Selection only borrows or clones original source statements.
                    // Admit the live selected sites, not an offset-only identity.
                    let _source_owner = context.source_declarations(
                        &namespace,
                        &workspace.sources[id].sha256,
                        document.target,
                        body,
                        None,
                    );
                    let mut scope = Scopes::new(body, context.clone(), namespace);
                    scope.set_terminal_none_source(&workspace.sources[id].text);
                    // Export events need original suite boundaries, including top-level
                    // assert truncation, even when declarations use a selected view.
                    scope.set_module_export_plan(&document.parsed.syntax().body);
                    if module.name == "ast"
                        && super::super::dependencies::embedded::is_source(&workspace.sources[id])
                    {
                        scope.complete_embedded_ast_constant();
                    }
                    if super::super::dependencies::embedded::is_source(&workspace.sources[id])
                        && crate::infrastructure::embedded::module(&module.name)
                            .is_some_and(|text| text == workspace.sources[id].text.as_str())
                    {
                        context.register_embedded_source_module(&scope.namespace);
                    }
                    scope
                })
            })
            .collect();
        (declarations, scopes)
    }

    pub(in crate::infrastructure::mypy) fn new(
        body: &'a [Stmt],
        target: PythonTarget,
        select_embedded_declarations: bool,
    ) -> Self {
        // Keep the existing embedded selection behavior. Installed .py sources
        // use installed() instead; other modules keep borrowing their original body.
        // Scopes needs a contiguous suite; class suites use the same selector.
        Self(
            Vec::new(),
            if select_embedded_declarations {
                Cow::Owned(
                    statements::collect(body, target)
                        .into_iter()
                        .cloned()
                        .collect(),
                )
            } else {
                Cow::Borrowed(body)
            },
        )
    }

    fn installed(body: &'a [Stmt], target: PythonTarget) -> Self {
        if !body.iter().any(has_compound_declaration) {
            return Self(Vec::new(), Cow::Borrowed(body));
        }
        let selected = statements::collect_installed(body, target);
        if selected.len() == body.len()
            && selected
                .iter()
                .zip(body)
                .all(|(selected, original)| std::ptr::eq(*selected, original))
        {
            return Self(Vec::new(), Cow::Borrowed(body));
        }
        Self(
            Vec::new(),
            Cow::Owned(selected.into_iter().cloned().collect()),
        )
    }

    pub(in crate::infrastructure::mypy) fn body(&self) -> &[Stmt] {
        &self.1
    }
}

// Probe same-scope compound suites without entering nested lexical bodies.
fn has_declaration(body: &[Stmt]) -> bool {
    body.iter().any(|statement| {
        matches!(
            statement,
            Stmt::ClassDef(_)
                | Stmt::FunctionDef(_)
                | Stmt::Assign(_)
                | Stmt::AnnAssign(_)
                | Stmt::TypeAlias(_)
                | Stmt::Import(_)
                | Stmt::ImportFrom(_)
        ) || has_compound_declaration(statement)
    })
}

fn has_compound_declaration(statement: &Stmt) -> bool {
    match statement {
        Stmt::If(conditional) => {
            has_declaration(&conditional.body)
                || conditional
                    .elif_else_clauses
                    .iter()
                    .any(|clause| has_declaration(&clause.body))
        }
        Stmt::Try(tried) => {
            has_declaration(&tried.body)
                || tried.handlers.iter().any(|handler| {
                    let ExceptHandler::ExceptHandler(handler) = handler;
                    has_declaration(&handler.body)
                })
                || has_declaration(&tried.orelse)
                || has_declaration(&tried.finalbody)
        }
        _ => false,
    }
}
