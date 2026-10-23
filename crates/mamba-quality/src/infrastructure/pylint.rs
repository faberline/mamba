//! Native default-check port. Completion requires all enabled reference checks.
use crate::domain::{model::*, source::QualityWorkspace};
use directives::suppressed;
use mamba_language::application::quality_syntax::python_ast::{Expr, Stmt};
mod abstracts;
mod all_exports;
mod argument_order;
mod argument_order_names;
mod arguments;
mod assignment_walk;
mod async_context;
mod attribute_calls;
mod attribute_naming;
mod attribute_resolution;
mod bad_super;
mod base_rules;
mod basic_calls;
mod basic_effects;
mod basic_statements;
mod binding_history;
mod build_attributes;
mod build_failure;
mod build_slots;
mod builtin_members;
mod builtins;
mod callables;
mod calls;
mod chained_comparisons;
mod checker;
use checker::Checker;
mod class_resolution;
mod class_rules;
mod class_sources;
mod class_walk;
mod classes;
mod comparison_constants;
mod comparison_context;
mod comparison_graph;
mod comparison_graph_build;
mod comparison_graph_cycles;
mod comparison_graph_paths;
mod comparison_graph_render;
mod comparison_graph_rules;
mod comparison_inference;
mod comparisons;
mod condition_inference;
mod conditions;
mod constructor_calls;
mod context_frames;
mod context_inference;
mod context_managers;
mod context_values;
mod context_yields;
mod dataclass_fields;
mod dataclass_sites;
mod declaration_locations;
mod declaration_walk;
mod default_arguments;
mod defaults;
mod deprecated_calls;
mod deprecated_data;
mod deprecated_imports;
mod descriptor_assignments;
mod design;
mod dictionary_iteration;
mod directives;
mod display;
mod docstrings;
mod dunder_calls;
mod dunder_data;
mod enum_flags;
mod enum_members;
mod enum_rules;
mod exception_handlers;
mod exception_raises;
mod exception_values;
mod expressions;
mod flow_context;
mod flow_rules;
mod function_design;
mod function_members;
mod function_metrics;
mod function_rules;
mod function_sources;
mod generator_cleanup;
mod hashable_members;
mod implicit_boolean;
mod implicit_context;
mod import_cycles;
mod import_members;
mod import_order;
mod import_rules;
mod import_walk;
mod indentation;
mod isinstance;
mod iteration_index;
mod iteration_index_scan;
mod iteration_index_values;
mod iteration_protocols;
mod lambda_rules;
mod layout_tokens;
mod local_rules;
mod logging;
mod loop_closures;
mod loop_variables;
mod match_patterns;
mod member_access;
mod member_access_context;
mod member_access_scan;
mod message_registry;
mod metaclasses;
mod metadata_nodes;
mod method_contracts;
mod method_hidden;
mod method_signatures;
mod modified_iteration;
mod modules;
mod mro;
mod naming;
mod native_extensions;
mod native_guard_eval;
mod native_member_guards;
mod nested_blocks;
mod nested_minmax;
mod non_ascii_names;
mod operand_bindings;
mod operand_computed;
mod operand_inference;
mod operand_nodes;
mod parent_delegation;
mod pragma_events;
mod pragma_information;
mod pragma_parser;
mod private_fields;
mod private_methods;
mod protected_access;
mod reassignments;
mod receiver_assignment;
mod recommendation_containers;
mod recommendation_context;
mod recommendation_generators;
mod recommendation_get;
mod recommendation_iteration;
mod recommendation_split;
mod redefinitions;
mod refactoring_booleans;
mod refactoring_choices;
mod refactoring_conditionals;
mod repeated_iterator;
mod repeated_iterator_exits;
mod repeated_iterator_sites;
mod repeated_targets;
mod resolution;
mod resource_calls;
mod results;
mod return_ended;
mod scopes;
mod semantic_rules;
mod sequence_indices;
mod side_effect_assignments;
mod similarities;
mod similarity_groups;
mod similarity_lines;
mod simplifiable_conditions;
mod simplifiable_context;
mod simplifiable_numbers;
mod simplifiable_truth;
mod slot_assignments;
mod slot_declarations;
mod slot_inference;
mod slot_model;
mod slot_stores;
mod source_bindings;
mod source_layout;
mod source_qualification;
mod special_returns;
mod special_signatures;
mod statements;
mod stdlib_arguments;
mod stdlib_calls;
mod stdlib_decorators;
mod stdlib_facts;
mod stdlib_names;
mod stdlib_open;
mod stdlib_runtime;
mod stdlib_signatures;
mod stdlib_unittest;
mod stdlib_versions;
mod stop_iteration;
mod stop_sites;
mod string_constants;
mod string_format;
mod subscript_protocols;
mod super_calls;
mod suppression_information;
mod swap_variables;
mod syntax_errors;
#[cfg(test)]
mod tests;
mod threading;
mod timeout_calls;
mod token_layout;
mod trailing_tuples;
mod traversal;
mod type_parameter_usage;
mod typecheck_statements;
mod typevar_naming;
mod unary_operands;
mod unicode_source;
mod unpacking;
mod unpacking_bindings;
mod unpacking_collections;
mod unpacking_operations;
mod unpacking_values;
mod value_constructor;
mod value_metadata;
mod values;

fn docstring(body: &[Stmt]) -> bool {
    matches!(body.first(), Some(Stmt::Expr(expression)) if matches!(expression.value.as_ref(), Expr::StringLiteral(_)))
}

impl Checker<'_> {
    fn emit(&mut self, rule: &str, name: &str, message: &str, range: ruff_text_size::TextRange) {
        let mut diagnostic = super::diagnostics::at(
            self.source,
            QualityTool::Pylint,
            rule,
            message.into(),
            range,
        );
        if let Some(column) = self.declaration_locations.column(range) {
            diagnostic.location.end_column = Some(i64::from(column));
        }
        if !suppressed(&self.directives, rule, name, diagnostic.location.line) {
            self.diagnostics.push(diagnostic);
        } else {
            self.record_suppression(rule, name, diagnostic.location.line);
        }
    }
}

mod runner;
mod runner_entry_inputs;
mod runner_read_errors;

pub fn check(workspace: &QualityWorkspace) -> ToolReport {
    check_with_logs(workspace, &super::failure_logs::FailureLogs::new(false))
}

pub(super) fn check_with_logs(
    workspace: &QualityWorkspace,
    logs: &super::failure_logs::FailureLogs,
) -> ToolReport {
    let mut report = runner::check(workspace, logs);
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
    report
}
