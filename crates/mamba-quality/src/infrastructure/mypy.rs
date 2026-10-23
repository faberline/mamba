//! Native strict-type port. Unimplemented inference remains an incomplete result.
mod classes;
mod declarations;
mod dependencies;
mod modules;
mod output;
mod output_order;
mod pydantic;
mod roots;
mod scopes;
mod signatures;
mod standard_collections;
mod types;
use crate::domain::{model::*, source::QualityWorkspace};
mod blockers;
mod checker;
mod directives;
#[cfg(test)]
mod tests;

pub fn check(workspace: &QualityWorkspace) -> ToolReport {
    let mut report = ToolReport { tool: QualityTool::Mypy, reference_version: "2.4.0".into(),
        completion: CompletionStatus::Incomplete, reference_exit_code: None, checked_files: 0, diagnostics: Vec::new(),
        remaining: vec!["mypy full strict inference is incomplete: symbol resolution, callable-value types, unpacked arguments, coroutine and generator results, containers, generics, overloads, Protocol, TypedDict, ParamSpec, TypeVarTuple, narrowing, inheritance, dataclass, and cross-file constraints".into(),
            "generic type aliases remain incomplete: recursive expansion, complete scopes, forward references, nested binders, ParamSpec, and TypeVarTuple".into(),
            "Pydantic 2.13.5 model construction, aliases, generic substitution, frozen fields, defaults, and dynamic-alias warnings remain incomplete".into()] };
    report
        .remaining
        .extend(workspace.projects.iter().flat_map(|project| {
            project
                .dependencies
                .iter()
                .filter(|dependency| dependency.kind == "unsupported-package-path")
                .map(|dependency| {
                    format!(
                        "package search is incomplete: {} ({})",
                        dependency.identity,
                        dependency
                            .path
                            .as_deref()
                            .unwrap_or(&project.root)
                            .display()
                    )
                })
        }));
    if let Some(diagnostic) = dependencies::core_shadow(workspace) {
        report.diagnostics.push(diagnostic);
        report.completion = CompletionStatus::Error;
        report.reference_exit_code = Some(2);
        return report;
    }
    let roots = match roots::prepare(workspace) {
        Ok(roots) => roots,
        Err(diagnostic) => {
            report.diagnostics.push(*diagnostic);
            report.completion = CompletionStatus::Error;
            report.reference_exit_code = Some(2);
            return report;
        }
    };
    let roots::Prepared { workspace, names } = roots;
    let checked_source_count = workspace.sources.len();
    let loaded = dependencies::load(workspace, names);
    let blocking = blockers::syntax(&loaded);
    if !blocking.is_empty() {
        report.diagnostics = blocking;
        report.completion = CompletionStatus::Error;
        report.reference_exit_code = Some(2);
        return report;
    }
    if let Some(diagnostic) = &loaded.blocking {
        report.diagnostics.push(diagnostic.clone());
        report.completion = CompletionStatus::Error;
        report.reference_exit_code = Some(2);
        return report;
    }
    let context = std::rc::Rc::new(types::Context::default());
    let index = loaded.index();
    // Only loader-admitted typed installed .py sources get this declaration view.
    let installed_sources: std::collections::BTreeSet<_> = (checked_source_count
        ..loaded.workspace.sources.len())
        .filter(|&id| {
            !loaded.local.contains(&id)
                && !loaded.workspace.sources[id].is_stub
                && !dependencies::embedded::is_source(&loaded.workspace.sources[id])
        })
        .collect();
    let modules = modules::Modules::build(
        &index,
        context.clone(),
        &loaded.workspace,
        loaded.untyped.clone(),
        loaded.imported.clone(),
        installed_sources,
    );
    let mut import_notes = std::collections::BTreeMap::new();
    let mut diagnostic_blocks = Vec::new();
    let mut export_gaps = std::collections::BTreeSet::new();
    for &id in &index.order {
        let checked = id < checked_source_count;
        if !checked && !loaded.local.contains(&id) {
            continue;
        }
        let source = &loaded.workspace.sources[id];
        if let Some(issue) = &source.decoding_issue {
            report.completion = CompletionStatus::Error;
            report
                .remaining
                .push(format!("{}: {}", source.path.display(), issue.message));
            continue;
        }
        match &index.modules[id].document {
            Some(document) => {
                if directives::ignore_errors(document) {
                    report.checked_files += usize::from(checked);
                    continue;
                }
                let lookup = modules::Modules::lookup(&modules, id);
                let body = &document.parsed.syntax().body;
                let _source_owner = context.source_declarations(
                    &lookup.namespace(),
                    &source.sha256,
                    document.target,
                    body,
                    None,
                );
                #[cfg(test)]
                let observation_namespace = lookup.namespace();
                let mut checker = checker::Checker::new(
                    source,
                    document,
                    context.clone(),
                    lookup,
                    import_notes.entry(source.project).or_default(),
                );
                checker.visit_block(body);
                #[cfg(test)]
                test_observation::observe(
                    &source.path,
                    &source.sha256,
                    document.target,
                    body,
                    &observation_namespace,
                    &context,
                );
                report.remaining.extend(checker.take_partial_list_gaps());
                export_gaps.extend(checker.export_state_gaps());
                report.checked_files += usize::from(checked);
                diagnostic_blocks.push((id, checker.finish()));
            }
            _ => report.remaining.push(format!(
                "{}: valid target-version syntax is required for native type inference",
                source.path.display()
            )),
        }
    }
    report.remaining.extend(export_gaps);
    output::append(&mut report, &loaded, &index, diagnostic_blocks);
    if !loaded.read_errors.is_empty() {
        report.completion = CompletionStatus::Error;
        report.remaining.extend(loaded.read_errors);
    }
    report
}

#[cfg(test)]
pub(crate) mod test_observation {
    use super::{classes::MethodKind, types};
    use mamba_language::application::quality_syntax::{python_ast::Stmt, PythonTarget};
    use ruff_text_size::Ranged;
    use std::{
        cell::RefCell,
        path::{Path, PathBuf},
        rc::Rc,
    };

    #[derive(Debug)]
    pub(crate) struct Receipt {
        pub source_identity: bool,
        pub exact_tuple_t: bool,
        pub fresh_returned_u: bool,
        pub rendered_returned_callable: Option<String>,
    }

    struct Probe {
        path: PathBuf,
        sha256: String,
        receipt: Option<Result<Receipt, String>>,
    }

    thread_local! {
        static PROBE: RefCell<Option<Probe>> = const { RefCell::new(None) };
    }

    pub(crate) struct Guard;

    pub(crate) fn arm(path: &Path, sha256: &str) -> Guard {
        PROBE.with(|slot| {
            let mut slot = slot.borrow_mut();
            assert!(slot.is_none(), "one public method receipt probe per thread");
            *slot = Some(Probe {
                path: path.to_path_buf(),
                sha256: sha256.to_owned(),
                receipt: None,
            });
        });
        Guard
    }

    impl Guard {
        pub(crate) fn take(&self) -> Option<Result<Receipt, String>> {
            PROBE.with(|slot| {
                slot.borrow_mut()
                    .as_mut()
                    .and_then(|probe| probe.receipt.take())
            })
        }
    }

    impl Drop for Guard {
        fn drop(&mut self) {
            PROBE.with(|slot| {
                slot.borrow_mut().take();
            });
        }
    }

    pub(super) fn observe(
        path: &Path,
        sha256: &str,
        target: PythonTarget,
        body: &[Stmt],
        namespace: &str,
        context: &Rc<types::Context>,
    ) {
        PROBE.with(|slot| {
            let mut slot = slot.borrow_mut();
            let Some(probe) = slot.as_mut() else { return };
            if probe.path.as_path() != path || probe.sha256 != sha256 {
                return;
            }
            assert!(probe.receipt.is_none(), "public method source checked once");
            probe.receipt = Some(inspect(target, body, namespace, sha256, context));
        });
    }

    fn inspect(
        target: PythonTarget,
        body: &[Stmt],
        namespace: &str,
        sha256: &str,
        context: &types::Context,
    ) -> Result<Receipt, String> {
        let class = body
            .iter()
            .find_map(|statement| match statement {
                Stmt::ClassDef(class) if class.name.as_str() == "Probe" => Some(class),
                _ => None,
            })
            .ok_or("original Probe class AST is absent")?;
        let method_ast = class
            .body
            .iter()
            .find_map(|statement| match statement {
                Stmt::FunctionDef(function) if function.name.as_str() == "get" => Some(function),
                _ => None,
            })
            .ok_or("original Probe.get method AST is absent")?;
        let input_ast = method_ast
            .parameters
            .iter()
            .find_map(|parameter| parameter.annotation())
            .ok_or("original Probe.get input AST is absent")?;
        let source_identity = context.class_source_identity(class)
            == Some((
                namespace.to_owned(),
                sha256.to_owned(),
                format!("{target:?}"),
            ))
            && context.source_declaration_site(input_ast);
        let key = (namespace.to_owned(), class.name.start().to_u32());
        let id = *context
            .classes
            .borrow()
            .keys
            .get(&key)
            .ok_or("original Probe class is absent from final Context")?;
        let produced = context.class(id);
        if produced.name != "Probe" || produced.namespace != namespace || produced.offset != key.1 {
            return Err("final class does not match original Probe AST".into());
        }
        let method = produced
            .methods
            .get("get")
            .ok_or("final Probe.get is absent")?;
        if method.kind != MethodKind::Instance
            || method.signature.owner.as_deref() != Some("Probe")
            || method.signature.name.as_deref() != Some("get")
        {
            return Err("final Probe.get is not the instance method".into());
        }
        let outer_t = method.signature.variables.get("T").copied();
        let exact_tuple_t = outer_t.is_some_and(|outer_t| {
            matches!(context.node(outer_t).as_deref(), Some(types::Node::Variable(variable)) if variable.name == "T")
                && method.signature.parameters.iter().find(|parameter| parameter.name == "value")
                    .and_then(|parameter| parameter.ty)
                    .is_some_and(|input| matches!(context.node(input).as_deref(), Some(types::Node::Tuple(items)) if items.as_slice() == [outer_t]))
        });
        let source_u = method.signature.variables.get("U").copied();
        let returned_ty = method.signature.returns;
        let fresh_returned_u = source_u.is_some_and(|source_u| {
            matches!(context.node(source_u).as_deref(), Some(types::Node::Variable(variable)) if variable.name == "U")
            && returned_ty.is_some_and(|ty| matches!(context.node(ty).as_deref(), Some(types::Node::Callable(_))))
            && returned_ty.and_then(|ty| context.callable_signature(ty))
            .is_some_and(|returned| {
                let [bound_u] = returned.generic.as_ref() else { return false };
                matches!(context.node(*bound_u).as_deref(), Some(types::Node::Variable(variable)) if variable.name == "U")
                    && returned.parameters.len() == 1
                    && returned.parameters[0].ty == Some(*bound_u)
                    && returned.returns == Some(*bound_u)
                    && source_u != *bound_u
                    && !method.signature.generic.contains(bound_u)
            })
        });
        let rendered_returned_callable = if fresh_returned_u {
            returned_ty.map(|ty| context.revealed_name_for(ty, target))
        } else {
            None
        };
        Ok(Receipt {
            source_identity,
            exact_tuple_t,
            fresh_returned_u,
            rendered_returned_callable,
        })
    }
}
