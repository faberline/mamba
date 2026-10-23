use super::{pydantic, scopes, signatures, types};
use crate::domain::{model::*, source::SourceSnapshot};
use mamba_language::application::quality_syntax::{
    self,
    python_ast::{
        visitor::{walk_expr, walk_stmt, Visitor},
        Expr, Stmt,
    },
};
use ruff_text_size::Ranged;
use std::collections::{BTreeMap, BTreeSet};
mod alias_recursion;
mod aliases;
mod annotations;
mod builtin_annotations;
mod calls;
mod class_generics;
mod class_metadata;
mod containers;
mod dataclass;
mod declarations;
mod exception_handlers;
mod expressions;
mod flow;
mod formatted_strings;
mod functions;
mod generator_inputs;
mod import_assignments;
mod indexing;
mod inheritance;
mod iteration_diagnostics;
mod lambdas;
mod loops;
mod name_scopes;
mod names;
mod narrowing;
mod objects;
mod overloads;
mod overrides;
mod parameter_spec_declarations;
mod parameter_specs;
mod partial_defaultdicts;
mod partial_dicts;
mod partial_lists;
mod partial_sets;
mod patterns;
mod protocols;
mod report;
mod returns;
mod sequence_comparisons;
mod template_strings;
mod truthiness;
mod try_inputs;
mod type_defaults;
mod type_objects;
mod type_var_tuples;
mod typed_dict;
mod typed_dict_declarations;
mod typed_dict_methods;
mod valid_types;
mod variables;
mod variance;
mod withs;

#[derive(Clone, Copy)]
struct BareYieldStatement {
    // Identity is used only during this real statement visit, never dereferenced.
    expression: usize,
    owner: (usize, scopes::Kind, Option<usize>),
    function_depth: usize,
}

struct CheckedCallResult {
    call: usize,
    owner: (usize, scopes::Kind, Option<usize>),
    actual: types::Type,
    expected: Option<types::Type>,
}

pub(super) struct Checker<'a> {
    source: &'a SourceSnapshot,
    import_notes: &'a mut std::collections::BTreeSet<String>,
    import_note_depths: BTreeMap<u32, Option<usize>>,
    missing_import_modules: BTreeSet<String>,
    python_target: quality_syntax::PythonTarget,
    diagnostics: Vec<QualityDiagnostic>,
    settled_diagnostics: Vec<std::ops::Range<usize>>,
    functions: Vec<Option<types::Type>>,
    scopes: scopes::Scopes,
    in_class: bool,
    classes: Vec<String>,
    class_types: Vec<Option<types::Type>>,
    constructor_none_plans: Vec<Option<super::classes::ConstructorNonePlan>>,
    constructor_none_frames: Vec<Option<super::classes::ConstructorNoneFrame>>,
    models: BTreeMap<String, pydantic::Model>,
    instances: Vec<BTreeMap<String, String>>,
    annotations: Vec<BTreeMap<String, ruff_text_size::TextSize>>,
    final_initializers: std::collections::BTreeSet<ruff_text_size::TextSize>,
    untyped_pattern_bindings: std::collections::BTreeSet<ruff_text_size::TextSize>,
    final_constants: BTreeMap<String, Expr>,
    invalid_alias_uses: std::collections::BTreeSet<(u32, usize, usize)>,
    invalid_alias_annotations: BTreeMap<ruff_text_size::TextSize, String>,
    directives: super::directives::TypeIgnores,
    type_comments: declarations::TypeComments,
    none_allowed: Option<ruff_text_size::TextRange>,
    bare_yield_statement: Option<BareYieldStatement>,
    context: std::rc::Rc<types::Context>,
    expected_value: Option<(ruff_text_size::TextRange, types::Type)>,
    type_parameter_declaration_call: Option<ruff_text_size::TextRange>,
    expected_calls: BTreeMap<(ruff_text_size::TextSize, ruff_text_size::TextSize), types::Type>,
    typed_dict_trial_failed: Option<bool>,
    checked_expression_types: Option<types::ObservedExpressionTypes>,
    checked_expression_owner: Option<(usize, scopes::Kind, Option<usize>)>,
    checked_expression_contexts: BTreeMap<usize, Option<types::Type>>,
    contextual_candidate_depth: usize,
    provisional_generator_context: bool,
    checked_normal_visits: BTreeSet<usize>,
    checked_call_result: Option<CheckedCallResult>,
    checked_source_call: Option<usize>,
    contextual_overload_gaps: BTreeSet<(u32, u32, &'static str)>,
    annotation_in_signature: bool,
    annotation_allow_component: bool,
    parameter_spec_errors: std::collections::BTreeSet<types::Type>,
    flow: flow::Flow,
    partial_sets: BTreeMap<ruff_text_size::TextSize, partial_sets::PartialSet>,
    partial_defaultdicts:
        BTreeMap<ruff_text_size::TextSize, partial_defaultdicts::PartialDefaultdict>,
    partial_defaultdict_deferred: BTreeSet<(ruff_text_size::TextSize, ruff_text_size::TextSize)>,
    partial_defaultdict_gaps: BTreeSet<String>,
    partial_set_scope: usize,
    next_partial_set_scope: usize,
    partial_dict_dynamic: bool,
    partial_dict_lvalues: std::collections::BTreeSet<ruff_text_size::TextSize>,
    partial_list_lvalues: std::collections::BTreeSet<ruff_text_size::TextSize>,
    partial_list_unknown: std::collections::BTreeSet<ruff_text_size::TextSize>,
    partial_list_gaps: std::collections::BTreeSet<String>,
    loops: Vec<loops::LoopFrame>,
    iteration_diagnostics: Vec<iteration_diagnostics::IterationDiagnostics>,
    loop_gaps: BTreeSet<String>,
    try_exits: Vec<exception_handlers::ExitFrame>,
    try_inputs: try_inputs::TryInputs,
    suppress_comparison_overlap: bool,
    template_depth: usize,
    fstring_observed: formatted_strings::Observations,
    _returned_callable_source: types::BorrowedReturnedCallableSourceGuard<'a>,
}

impl<'a> Checker<'a> {
    pub(super) fn export_state_gaps(&self) -> Vec<String> {
        let mut gaps = self.scopes.export_state_gaps();
        gaps.extend(self.scopes.returned_callable_gaps());
        gaps.extend(self.loop_gaps.iter().cloned());
        gaps.extend(self.contextual_overload_gaps.iter().map(|(start, end, reason)| {
            format!("{}: contextual overload argument at bytes {start}..{end} remains unsupported/unknown: {reason}", self.source.path.display())
        }));
        gaps.extend(self.partial_defaultdict_gaps.iter().cloned());
        gaps.extend(
            self.context
                .classes
                .borrow()
                .constructor_none_gaps
                .iter()
                .cloned(),
        );
        // A shared Context records actual unproved paths without a site claim.
        if self.context.has_unsupported_any_origin() {
            gaps.push("runtime-erased Any origins remain unsupported/unknown for opaque or native-only joins and alias/variable meets; their ordinary Any fallback is not proved original behavior".into());
        }
        gaps
    }

    pub(super) fn new(
        source: &'a SourceSnapshot,
        document: &'a quality_syntax::SyntaxDocument<'_>,
        context: std::rc::Rc<types::Context>,
        lookup: super::modules::Lookup,
        import_notes: &'a mut std::collections::BTreeSet<String>,
    ) -> Self {
        let returned_callable_source = context.borrow_returned_callable_source(
            &lookup.namespace(),
            &source.sha256,
            document.target,
            &document.parsed.syntax().body,
        );
        let mut scopes = scopes::Scopes::new(
            &document.parsed.syntax().body,
            context.clone(),
            lookup.namespace(),
        );
        scopes.set_terminal_none_source(&source.text);
        scopes.set_module_export_plan(&document.parsed.syntax().body);
        scopes.attach_modules(lookup, &document.parsed.syntax().body);
        let models = pydantic::collect(
            &document.parsed.syntax().body,
            &context,
            &scopes.variables(),
        );
        let final_constants = calls::qualifiers::literals::module_constants(
            &document.parsed.syntax().body,
            &context,
            &scopes.variables(),
        );
        let mut checker = Self {
            source,
            import_notes,
            import_note_depths: BTreeMap::new(),
            missing_import_modules: BTreeSet::new(),
            python_target: document.target,
            diagnostics: Vec::new(),
            settled_diagnostics: Vec::new(),
            functions: Vec::new(),
            suppress_comparison_overlap: false,
            template_depth: 0,
            fstring_observed: Default::default(),
            _returned_callable_source: returned_callable_source,
            scopes,
            in_class: false,
            classes: Vec::new(),
            class_types: Vec::new(),
            constructor_none_plans: Vec::new(),
            constructor_none_frames: Vec::new(),
            models,
            instances: vec![BTreeMap::new()],
            annotations: vec![BTreeMap::new()],
            final_initializers: std::collections::BTreeSet::new(),
            untyped_pattern_bindings: Default::default(),
            final_constants,
            invalid_alias_uses: std::collections::BTreeSet::new(),
            invalid_alias_annotations: BTreeMap::new(),
            directives: super::directives::TypeIgnores::from_document(document),
            type_comments: declarations::TypeComments::collect(document),
            none_allowed: None,
            bare_yield_statement: None,
            context,
            expected_value: None,
            type_parameter_declaration_call: None,
            expected_calls: BTreeMap::new(),
            typed_dict_trial_failed: None,
            checked_expression_types: None,
            checked_expression_owner: None,
            checked_expression_contexts: Default::default(),
            contextual_candidate_depth: 0,
            provisional_generator_context: false,
            checked_normal_visits: Default::default(),
            checked_call_result: None,
            checked_source_call: None,
            contextual_overload_gaps: Default::default(),
            annotation_in_signature: false,
            annotation_allow_component: false,
            parameter_spec_errors: Default::default(),
            flow: flow::Flow::default(),
            partial_sets: BTreeMap::new(),
            partial_defaultdicts: BTreeMap::new(),
            partial_defaultdict_deferred: BTreeSet::new(),
            partial_defaultdict_gaps: BTreeSet::new(),
            partial_set_scope: 0,
            next_partial_set_scope: 1,
            partial_dict_dynamic: false,
            partial_dict_lvalues: Default::default(),
            partial_list_lvalues: Default::default(),
            partial_list_unknown: Default::default(),
            partial_list_gaps: Default::default(),
            loops: Vec::new(),
            iteration_diagnostics: Vec::new(),
            loop_gaps: BTreeSet::new(),
            try_exits: Vec::new(),
            try_inputs: Default::default(),
        };
        checker.prepare_template_strings(document);
        checker
    }

    fn infer(&self, expression: &Expr) -> Option<types::Type> {
        let environment = self
            .scopes
            .inference_environment(expression, self.environment());
        let infer = || {
            self.context
                .infer_with_calls(expression, &environment, &|call| {
                    self.scopes.expression_type(call, &environment)
                })
                .or_else(|| self.untyped_pattern_value(expression))
        };
        if self.checked_expression_owner == Some(self.scopes.try_input_scope())
            && (self.contextual_candidate_depth != 0
                || !self.context.has_observed_expression_types())
        {
            if let Some(observed) = &self.checked_expression_types {
                return self.context.with_checked_expression_types(observed, infer);
            }
        }
        infer()
    }

    fn model_instance(&self, name: &str) -> Option<pydantic::Model> {
        if let Some(ty) = self
            .environment()
            .get(name)
            .copied()
            .filter(|&ty| !self.context.is_class_value(ty))
        {
            if let Some(id) = self.context.class_id(ty) {
                if let Some(model) = self.context.class(id).pydantic.clone() {
                    return Some(model);
                }
            }
        }
        let model = self
            .instances
            .iter()
            .rev()
            .find_map(|scope| scope.get(name))?;
        self.models.get(model).cloned()
    }

    fn remember(&mut self, target: &Expr, value: &Expr) {
        if let Expr::Name(variable) = target {
            self.remember_name(variable.id.as_str(), value);
        }
    }

    fn remember_name(&mut self, variable: &str, value: &Expr) {
        let model = match value {
            Expr::Call(call) => match call.func.as_ref() {
                Expr::Name(name) => Some(name.id.as_str()),
                _ => None,
            },
            Expr::Name(name) => Some(name.id.as_str()),
            _ => None,
        };
        let current = self.instances.last_mut().expect("module instance scope");
        if let Some(model) = model.filter(|model| self.models.contains_key(*model)) {
            current.insert(variable.into(), model.into());
        } else {
            current.remove(variable);
        }
    }
}

impl Checker<'_> {
    fn visit_statement<'a>(&mut self, statement: &'a Stmt)
    where
        Self: Visitor<'a>,
    {
        if let Stmt::Assign(assignment) = statement {
            self.check_inherited_class_assignment(assignment);
        }
        if self.scopes.alias_definition(statement) {
            self.check_type_alias_declaration(statement);
            return;
        }
        match statement {
            Stmt::ClassDef(class) if self.check_class_import_redefinition(class) => {}
            Stmt::ClassDef(class) => {
                self.check_special_class_attributes(class);
                self.check_class_type_defaults(class);
                self.check_dataclass_declaration(class);
                if self
                    .scopes
                    .class_object(class.name.as_str())
                    .and_then(|ty| self.context.typed_dict(ty))
                    .is_some()
                {
                    self.check_typed_dict_declaration(class);
                    return;
                }
                self.check_protocol_declaration(class);
                let annotation_class = self
                    .scopes
                    .class_object(class.name.as_str())
                    .and_then(|ty| self.context.class_id(ty));
                let constructor_none_plan = annotation_class.and_then(|id| {
                    self.scopes
                        .constructor_none_plan(id, class, &self.source.sha256)
                });
                let variables = annotation_class.map_or_else(
                    || self.scopes.variables(),
                    |id| {
                        self.context
                            .class_scope_variables(id, self.scopes.variables())
                    },
                );
                let bound = annotation_class.map_or_else(Default::default, |id| {
                    self.context.class(id).generic.iter().copied().collect()
                });
                let previous_annotation_scope = self.scopes.annotation_scope(variables, bound);
                self.check_bases(class);
                self.scopes
                    .annotation_scope(previous_annotation_scope.0, previous_annotation_scope.1);
                self.check_inherited_methods(class);
                let previous_inputs = std::mem::take(&mut self.try_inputs);
                let previous_flow = std::mem::take(&mut self.flow);
                let previous_partial_set_scope = self.enter_partial_set_scope();
                let previous = self.in_class;
                self.in_class = true;
                self.classes.push(class.name.to_string());
                self.class_types
                    .push(self.scopes.class_object(class.name.as_str()));
                self.constructor_none_plans.push(constructor_none_plan);
                let class_bound = self
                    .class_types
                    .last()
                    .copied()
                    .flatten()
                    .and_then(|ty| self.context.class_id(ty))
                    .map(|id| self.context.class(id).generic.to_vec())
                    .unwrap_or_default();
                self.scopes.enter_proved_class(
                    class,
                    &self.source.sha256,
                    self.python_target,
                    BTreeMap::new(),
                    self.scopes
                        .class_object(class.name.as_str())
                        .and_then(|ty| self.context.class_id(ty))
                        .map_or_else(
                            || self.scopes.variables(),
                            |id| {
                                self.context
                                    .class_scope_variables(id, self.scopes.variables())
                            },
                        ),
                    &class_bound,
                );
                self.check_model_fields(class);
                self.check_overload_groups(&class.body);
                self.annotations.push(BTreeMap::new());
                walk_stmt(self, statement);
                self.finish_partial_sets();
                self.annotations.pop();
                self.classes.pop();
                self.class_types.pop();
                self.constructor_none_plans.pop();
                self.scopes.leave();
                self.partial_set_scope = previous_partial_set_scope;
                self.in_class = previous;
                self.flow = previous_flow;
                self.try_inputs = previous_inputs;
            }
            Stmt::FunctionDef(function) => self.visit_function(function),
            Stmt::Import(_) | Stmt::ImportFrom(_) => {
                self.check_import_assignments(statement);
                let mut no_event = self.scopes.bind_checked_imports(statement);
                for (range, diagnostic) in self
                    .scopes
                    .import_diagnostics(std::slice::from_ref(statement))
                {
                    no_event = false;
                    self.emit_import_diagnostic(range, diagnostic, statement);
                }
                self.complete_try_import(statement, no_event);
            }
            Stmt::Return(returned) => {
                let previous = self.none_allowed;
                let expected = returned
                    .value
                    .as_deref()
                    .zip(self.functions.last().copied().flatten());
                let previous_expected = self.expected_value;
                self.expected_value = expected.map(|(value, expected)| (value.range(), expected));
                if matches!(
                    self.functions.last(),
                    Some(Some(
                        types::Type::None | types::Type::Any | types::Type::AnySpecial
                    ))
                ) {
                    self.none_allowed = returned.value.as_deref().map(Ranged::range);
                }
                walk_stmt(self, statement);
                self.none_allowed = previous;
                self.expected_value = previous_expected;
                self.check_return(returned);
                self.record_return_exit();
                self.flow.reachable = false;
            }
            Stmt::Try(tried) => self.check_try(tried),
            Stmt::If(conditional) => self.check_if(conditional),
            Stmt::Match(matched) => self.check_match(matched),
            Stmt::For(looped) => self.check_for(looped),
            Stmt::While(looped) => self.check_while(looped),
            Stmt::With(with) if !with.is_async => self.check_with(with),
            Stmt::Break(_) => self.check_break(),
            Stmt::Continue(_) => self.check_continue(),
            Stmt::Assert(assertion) => {
                walk_stmt(self, statement);
                let before = self.flow.clone();
                self.flow_condition(&assertion.test, true);
                self.record_failed_assert(before);
            }
            Stmt::Raise(raised) => {
                walk_stmt(self, statement);
                self.record_raise_exit(raised.exc.as_deref());
                self.flow.reachable = false;
            }
            Stmt::AnnAssign(assignment) => {
                if self.reject_late_local_annotation(assignment) {
                    return;
                }
                self.check_inherited_class_annotation(assignment);
                self.remember_final_initializer(assignment);
                let previous = self.expected_value;
                self.expected_value = assignment
                    .value
                    .as_deref()
                    .zip(self.scopes.annotation(&assignment.annotation))
                    .map(|(value, expected)| (value.range(), expected));
                walk_stmt(self, statement);
                self.expected_value = previous;
                if !self.check_module_global_annotation(assignment) {
                    self.check_annotation(assignment);
                }
            }
            Stmt::Assign(assignment) => {
                self.check_legacy_type_default(assignment);
                if self.scopes.functional_class_binding(assignment) {
                    self.check_functional_typed_dict(assignment);
                    return;
                }
                let previous_declaration_call = self.type_parameter_declaration_call;
                self.type_parameter_declaration_call =
                    match (assignment.targets.as_slice(), assignment.value.as_ref()) {
                        ([Expr::Name(_)], Expr::Call(call)) => {
                            let plain_arguments = call
                                .arguments
                                .args
                                .iter()
                                .all(|argument| !matches!(argument, Expr::Starred(_)));
                            let owned_shape = match self
                                .context
                                .typing_form(&call.func, &self.scopes.variables())
                                .as_deref()
                            {
                                Some("ParamSpec") => {
                                    call.arguments.args.len() == 1
                                        && call.arguments.keywords.iter().all(|keyword| {
                                            matches!(
                                                keyword.arg.as_ref().map(|name| name.as_str()),
                                                Some(
                                                    "bound"
                                                        | "covariant"
                                                        | "contravariant"
                                                        | "default"
                                                )
                                            )
                                        })
                                }
                                Some("TypeVarTuple") => call
                                    .arguments
                                    .keywords
                                    .iter()
                                    .all(|keyword| keyword.arg.is_some()),
                                _ => false,
                            };
                            (plain_arguments && owned_shape).then_some(call.range())
                        }
                        _ => None,
                    };
                let previous = self.expected_value;
                self.expected_value = assignment
                    .targets
                    .first()
                    .and_then(|target| self.scopes.assigned_type(target))
                    .map(|expected| (assignment.value.range(), expected));
                let partial_dict_lvalues = self.partial_dict_assignment_receivers(assignment);
                self.partial_dict_lvalues
                    .extend(partial_dict_lvalues.iter().copied());
                walk_stmt(self, statement);
                for location in partial_dict_lvalues {
                    self.partial_dict_lvalues.remove(&location);
                }
                self.type_parameter_declaration_call = previous_declaration_call;
                self.expected_value = previous;
                self.check_assignment(assignment);
            }
            Stmt::Expr(expression) => self.visit_expression_statement(expression),
            Stmt::AugAssign(assignment) => {
                walk_stmt(self, statement);
                self.flag_partial_list_augmented(assignment);
            }
            Stmt::Delete(deleted) => {
                for target in &deleted.targets {
                    self.visit_expr(target);
                    self.delete_name(target);
                }
            }
            _ => walk_stmt(self, statement),
        }
    }
}

impl<'a> Visitor<'a> for Checker<'_> {
    fn visit_stmt(&mut self, statement: &'a Stmt) {
        // A real statement visit owns this lease, including its assignment check.
        // Nested statements and loop passes never inherit the previous visit's map.
        let previous_checked = self.checked_expression_types.take();
        let previous_checked_owner = self.checked_expression_owner.take();
        let previous_checked_contexts = std::mem::take(&mut self.checked_expression_contexts);
        let previous_checked_visits = std::mem::take(&mut self.checked_normal_visits);
        let previous_bare_yield = self.bare_yield_statement.take();
        if let Stmt::Expr(statement) = statement {
            if matches!(statement.value.as_ref(), Expr::Yield(value) if value.value.is_none()) {
                self.bare_yield_statement = Some(BareYieldStatement {
                    expression: statement.value.as_ref() as *const Expr as usize,
                    owner: self.scopes.try_input_scope(),
                    function_depth: self.functions.len(),
                });
            }
        }
        let writer = self.begin_try_writer(statement);
        self.visit_statement(statement);
        self.finish_try_writer(writer);
        self.bare_yield_statement = previous_bare_yield;
        self.checked_expression_types = previous_checked;
        self.checked_expression_owner = previous_checked_owner;
        self.checked_expression_contexts = previous_checked_contexts;
        self.checked_normal_visits = previous_checked_visits;
    }

    fn visit_expr(&mut self, expression: &'a Expr) {
        if !self
            .checked_normal_visits
            .insert(expression as *const Expr as usize)
        {
            // Only actual normal visits are counted. Candidate checks never
            // populate this set. Invalidate before any normal hook can infer.
            self.checked_expression_types = None;
            self.checked_expression_owner = None;
            self.checked_expression_contexts.clear();
        }
        let unproved_suspend = match expression {
            Expr::Yield(value) if value.value.is_none() => {
                !self.proved_bare_yield_statement(expression)
            }
            Expr::Named(_) | Expr::Yield(_) | Expr::YieldFrom(_) | Expr::Await(_) => true,
            _ => false,
        };
        if unproved_suspend {
            self.refuse_try_inputs();
        }
        let call_exit = self.call_exit_marker();
        let reachable = self.flow.reachable;
        self.observe_loop_expression(expression);
        let observed_environment = self.fstring_observed_environment();
        let partial_dict_update = self.partial_dict_update_receiver(expression);
        if let Some(location) = partial_dict_update {
            self.partial_dict_lvalues.insert(location);
        }
        if !self.visit_partial_list_call(expression) {
            self.visit_expression(expression);
        }
        if let Some(location) = partial_dict_update {
            self.partial_dict_lvalues.remove(&location);
            self.finish_partial_dict_update(expression);
        }
        if let Some(environment) = observed_environment {
            self.record_fstring_type(expression, &environment);
        }
        if reachable && !self.flow.reachable && matches!(expression, Expr::Call(_)) {
            self.record_nonreturning_call(call_exit);
        }
    }
}

impl Checker<'_> {
    fn proved_bare_yield_statement(&self, expression: &Expr) -> bool {
        // Mypy checkexpr.py:6348-6366 adds no binder event for bare yield.
        let Some(proof) = self.bare_yield_statement else {
            return false;
        };
        proof.expression == expression as *const Expr as usize
            && proof.owner == self.scopes.try_input_scope()
            && proof.owner.1 == scopes::Kind::Function
            && proof.owner.2.is_none()
            && proof.function_depth != 0
            && proof.function_depth == self.functions.len()
            && self
                .scopes
                .generator()
                .is_some_and(|(_, asynchronous)| !asynchronous)
    }
}
