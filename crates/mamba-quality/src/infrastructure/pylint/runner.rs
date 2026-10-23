//! Native Pylint runner retains immutable project snapshots and checker order.
mod checker;
mod imported_sources;
use super::{
    async_context, comparison_context, dataclass_sites, declaration_locations,
    dictionary_iteration, directives::instructions, docstring, dunder_calls, flow_context,
    function_metrics, implicit_context, import_cycles, lambda_rules, local_rules, loop_closures,
    loop_variables, modules, native_member_guards, nested_blocks, reassignments,
    recommendation_containers, recommendation_context, recommendation_generators,
    recommendation_get, redefinitions, repeated_iterator, resource_calls, scopes, similarities,
    simplifiable_context, slot_assignments, statements, stop_sites, suppression_information,
    swap_variables, syntax_errors, unicode_source, Checker,
};
use crate::domain::{model::*, source::QualityWorkspace};
use mamba_language::application::quality_syntax::python_ast::visitor::Visitor;
use std::collections::BTreeSet;

pub(super) fn check(
    workspace: &QualityWorkspace,
    logs: &super::super::failure_logs::FailureLogs,
) -> ToolReport {
    let mut report = ToolReport { tool: QualityTool::Pylint, reference_version: "4.1.2".into(),
        completion: CompletionStatus::Incomplete, reference_exit_code: None, checked_files: 0, diagnostics: Vec::new(),
        remaining: vec!["Pylint full default-check parity is incomplete: naming, members, calls, imports, control flow, refactoring, and cross-file inference".into()] };
    report.diagnostics = super::runner_entry_inputs::diagnostics(workspace);
    if !report.diagnostics.is_empty() {
        report.completion = CompletionStatus::Error;
    }
    let loaded = imported_sources::load(workspace);
    if !loaded.read_errors.is_empty() {
        report.completion = CompletionStatus::Error;
        report.remaining.extend(loaded.read_errors.iter().cloned());
    }
    let expanded = &loaded.workspace;
    let index = loaded.index();
    let module_tables: Vec<_> = workspace
        .projects
        .iter()
        .enumerate()
        .map(|(project, _)| modules::Modules::new(&index, expanded, project))
        .collect();
    // Astroid builds every selected source before checker traversal.
    let mut build_failures = BTreeSet::new();
    let mut build_attributes = std::collections::BTreeMap::new();
    for (id, source) in expanded
        .sources
        .iter()
        .enumerate()
        .filter(|(_, source)| !source.is_stub)
    {
        let module = &index.modules[id];
        let Some(document) = &module.document else {
            continue;
        };
        if source.decoding_issue.is_some()
            || unicode_source::parsing_failure(source).is_some()
            || unicode_source::token_failure(source, document).is_some()
        {
            continue;
        }
        let body = document.parsed.syntax().body.as_slice();
        let mut checker = checker::new(
            source,
            &index,
            &expanded.sources,
            module,
            &module_tables[source.project],
            body,
        );
        if checker.slot_build_failure(body) {
            build_failures.insert(id);
        }
        build_attributes.insert(id, checker.build_attributes);
    }
    for (id, source) in workspace
        .sources
        .iter()
        .enumerate()
        .filter(|(_, source)| !source.is_stub)
    {
        if let Some(diagnostic) = unicode_source::parsing_failure(source) {
            report.checked_files += 1;
            report.diagnostics.push(diagnostic);
            continue;
        }
        if let Some(issue) = &source.decoding_issue {
            report.completion = CompletionStatus::Error;
            report
                .remaining
                .push(format!("{}: {}", source.path.display(), issue.message));
            continue;
        }
        let module = &index.modules[id];
        let document = module.document.as_ref();
        report.checked_files += 1;
        if let Some(diagnostic) =
            document.and_then(|document| unicode_source::token_failure(source, document))
        {
            report.diagnostics.push(diagnostic);
            continue;
        }
        if document.is_none() {
            if let Some(diagnostic) =
                syntax_errors::diagnostic(source, module_tables[source.project].target())
            {
                report.diagnostics.push(diagnostic);
                continue;
            }
        }
        let body = document
            .map(|document| document.parsed.syntax().body.as_slice())
            .unwrap_or_default();
        let mut checker = checker::new(
            source,
            &index,
            &expanded.sources,
            module,
            &module_tables[source.project],
            body,
        );
        if build_failures.contains(&id) {
            report.completion = CompletionStatus::Error;
            match super::build_failure::diagnostic(source, workspace, logs) {
                Ok(diagnostic) => report.diagnostics.push(diagnostic),
                Err(error) => report.remaining.push(format!(
                    "{}: cannot save the Pylint build failure report: {error}",
                    source.path.display()
                )),
            }
            continue;
        }
        checker.build_attributes = build_attributes.remove(&id).unwrap_or_default();
        if checker.check_pragma_events() {
            checker.check_suppression_information();
            report.diagnostics.extend(checker.diagnostics);
            continue;
        }
        checker.check_unicode_source();
        if let Some(document) = document {
            checker.check_keyword_parentheses(document);
        }
        checker.check_source_layout(document);
        match document {
            Some(document) => {
                checker.check_string_tokens(document);
                checker.check_trailing_tuples(document);
                if !docstring(&document.parsed.syntax().body)
                    && !document.parsed.syntax().body.is_empty()
                {
                    checker.emit(
                        "C0114",
                        "missing-module-docstring",
                        "Missing module docstring",
                        super::super::diagnostics::point(0),
                    );
                }
                checker.check_builtin_shadowing();
                checker.check_non_ascii_name(
                    module.name.split('.').next_back().unwrap_or(&module.name),
                    "File",
                    super::super::diagnostics::point(0),
                );
                checker.check_empty_docstring(body, "module", super::super::diagnostics::point(0));
                checker.visit_body(&document.parsed.syntax().body);
                if let Some(error) = checker.check_import_order(body) {
                    report.completion = CompletionStatus::Error;
                    match super::build_failure::import_permission(source, &error, workspace, logs) {
                        Ok(diagnostic) => checker.diagnostics.push(diagnostic),
                        Err(error) => report.remaining.push(format!(
                            "{}: cannot save the Pylint import failure report: {error}",
                            source.path.display()
                        )),
                    }
                    report.diagnostics.extend(checker.diagnostics);
                    continue;
                }
                checker.check_exports();
                checker.check_unresolved_metaclasses(body);
                checker.leave_scope();
                checker.check_suppression_information();
            }
            _ => report.remaining.push(format!(
                "{}: valid target-version syntax is required for native Pylint checks",
                source.path.display()
            )),
        }
        report.diagnostics.extend(checker.diagnostics);
    }
    let selected_index = loaded.selected_index(workspace);
    report
        .diagnostics
        .extend(import_cycles::check(workspace, &selected_index));
    report
        .diagnostics
        .extend(similarities::check(workspace, &selected_index));
    if report.diagnostics.iter().any(|value| value.rule == "F0002") {
        report.reference_exit_code = Some(report.diagnostics.iter().fold(0, |exit, value| {
            exit | match value.severity.as_str() {
                "fatal" => 1,
                "error" => 2,
                "warning" => 4,
                "refactor" => 8,
                "convention" => 16,
                _ => 0,
            }
        }));
    }
    report
}
