use super::*;
use quality_syntax::python_ast::{StmtAnnAssign, StmtAssign, StmtReturn};
mod empty_conditionals;
mod inherited;
mod partial_none;
mod unpack;

struct AssignmentBinding<'a> {
    target: &'a Expr,
    expression: Option<&'a Expr>,
    ty: types::Type,
}

struct AssignmentMemory {
    callable: Option<std::sync::Arc<signatures::Signature>>,
    model: Option<String>,
}

struct PreparedAssignment<'a> {
    target: &'a Expr,
    actual: Option<types::Type>,
    memory: Option<AssignmentMemory>,
    compatible: bool,
}

impl Checker<'_> {
    pub(super) fn check_return(&mut self, returned: &StmtReturn) {
        if returned.value.is_some()
            && self
                .scopes
                .generator()
                .is_some_and(|(_, asynchronous)| asynchronous)
        {
            self.emit(
                "misc",
                "\"return\" with value in async generator is not allowed".into(),
                returned,
            );
            return;
        }
        let Some(Some(expected)) = self.functions.last().copied() else {
            return;
        };
        if expected == types::Type::Never {
            self.emit(
                "misc",
                "Return statement in function which does not return".into(),
                returned,
            );
            return;
        }
        let Some(value) = returned.value.as_deref() else {
            if !matches!(
                expected,
                types::Type::None | types::Type::Any | types::Type::AnySpecial
            ) {
                self.emit("return-value", "Return value expected".into(), returned);
            }
            return;
        };
        if let Some(actual) = self.infer_expected(value, expected) {
            if actual.is_any()
                && !matches!(
                    expected,
                    types::Type::Any | types::Type::AnySpecial | types::Type::Object
                )
            {
                self.emit(
                    "no-any-return",
                    format!(
                        "Returning Any from function declared to return \"{}\"",
                        self.context.name(expected)
                    ),
                    returned,
                );
                return;
            }
            if !self.context.accepts(expected, actual) {
                if expected == types::Type::None {
                    self.emit("return-value", "No return value expected".into(), returned);
                    return;
                }
                let hint = self.context.variance_hint(expected, actual);
                self.emit_with_hint(
                    "return-value",
                    format!(
                        "Incompatible return value type (got \"{}\", expected \"{}\")",
                        self.context.name(actual),
                        self.context.name(expected)
                    ),
                    value,
                    hint,
                );
            }
        }
    }

    pub(super) fn remember_final_initializer(&mut self, assignment: &StmtAnnAssign) {
        let annotation = if let Expr::Subscript(value) = assignment.annotation.as_ref() {
            value.value.as_ref()
        } else {
            assignment.annotation.as_ref()
        };
        if assignment.value.is_some()
            && self
                .context
                .typing_form(annotation, &self.scopes.variables())
                .as_deref()
                == Some("Final")
        {
            self.final_initializers.insert(assignment.target.start());
        }
    }

    pub(super) fn reject_late_local_annotation<'a>(&mut self, assignment: &'a StmtAnnAssign) -> bool
    where
        Self: Visitor<'a>,
    {
        let Some(previous) = self
            .scopes
            .prior_inferred_local_definition(&assignment.target, &assignment.annotation)
        else {
            return false;
        };
        let Expr::Name(name) = assignment.target.as_ref() else {
            return false;
        };
        self.report_annotation_redefinition(name, previous);
        // A rejected annotation supplies no expected type or binding/flow constraint.
        let previous_context = self.expected_value.take();
        if let Some(value) = assignment.value.as_deref() {
            self.visit_expr(value);
        }
        self.expected_value = previous_context;
        true
    }

    fn report_annotation_redefinition(
        &mut self,
        name: &quality_syntax::python_ast::ExprName,
        previous: ruff_text_size::TextSize,
    ) {
        let line = self.source.text[..usize::from(previous)]
            .bytes()
            .filter(|&byte| byte == b'\n')
            .count()
            + 1;
        self.emit(
            "no-redef",
            format!("Name \"{}\" already defined on line {line}", name.id),
            name,
        );
    }

    pub(super) fn check_annotation(&mut self, assignment: &StmtAnnAssign) {
        self.remember_invalid_alias_annotation(assignment);
        if let Expr::Name(name) = assignment.target.as_ref() {
            let scope = self
                .annotations
                .last_mut()
                .expect("module annotation scope");
            if let Some(previous) = scope.get(name.id.as_str()).copied() {
                self.report_annotation_redefinition(name, previous);
                return;
            }
            scope.insert(name.id.to_string(), name.start());
        }
        self.check_callable_annotation(&assignment.annotation);
        if self.scopes.annotation(&assignment.annotation).is_none() {
            if let Some(value) = assignment
                .value
                .as_deref()
                .filter(|value| self.scopes.annotation(value).is_some())
            {
                self.check_callable_annotation(value);
            }
        }
        self.remember(&assignment.target, &assignment.annotation);
        let expected = self.scopes.annotation(&assignment.annotation);
        self.prepare_try_assignment(&assignment.target, None);
        self.scopes.declare(&assignment.target, expected);
        let (Some(expected), Some(value)) = (expected, assignment.value.as_deref()) else {
            return;
        };
        if self.source.is_stub && matches!(value, Expr::EllipsisLiteral(_)) {
            return;
        }
        if let Some(actual) = self.infer_expected(value, expected) {
            let checked = self.check_assignment_type(expected, actual, value);
            // An initializer establishes the declaration's type. Later assignments narrow it.
            let compatible = self.context.accepts(expected, actual);
            if assignment.target.is_name_expr() && checked {
                self.complete_try_no_event(&assignment.target);
            }
            if !matches!(assignment.target.as_ref(), Expr::Name(_)) {
                let initialized = if compatible { expected } else { actual };
                self.flow_assignment(&assignment.target, Some(initialized));
                self.record_try_input_commit(&assignment.target, Some(initialized), compatible);
            }
        }
    }

    pub(super) fn check_assignment(&mut self, assignment: &StmtAssign) {
        if self.functions.is_empty()
            && self.classes.is_empty()
            && matches!(assignment.targets.as_slice(), [target @ Expr::Name(_)] if self.scopes.assigned_type(target).is_none())
            && matches!(assignment.value.as_ref(), Expr::Name(name) if name.id.as_str() == "tuple" && !self.scopes.has_binding("tuple"))
        {
            self.emit(
                "type-arg",
                "Missing type arguments for generic type \"tuple\"".into(),
                &ruff_text_size::TextRange::at(
                    assignment.value.start(),
                    ruff_text_size::TextSize::new(1),
                ),
            );
        }
        let actual = self.infer_list_common_assignment(assignment);
        let ordinary_actual = actual;
        let nested_empty_set = match assignment.targets.as_slice() {
            [target @ Expr::Name(name)]
                if self.flow.reachable
                    && name.id.as_str() != "_"
                    && matches!(assignment.value.as_ref(), Expr::DictComp(_))
                    && self.scopes.assigned_type(target).is_none()
                    && self
                        .scopes
                        .first_inferred_local_assignment(target)
                        .is_some_and(|(_, first)| first == name.start()) =>
            {
                self.scopes.recover_empty_set_comprehension(
                    &assignment.value,
                    actual,
                    &self.environment(),
                )
            }
            _ => None,
        };
        let partial_defaultdict = self.partial_defaultdict_assignment_seed(assignment);
        self.prepare_partial_defaultdict_assignment(assignment, actual);
        let empty_conditional = self.inferred_empty_conditional_assignment(assignment, actual);
        let actual = empty_conditional.or(actual);
        self.prepare_partial_dict_assignment(assignment, actual);
        for target in &assignment.targets {
            self.prepare_partial_list_assignment(target, &assignment.value, actual);
        }
        let preserve_bottom = self
            .scopes
            .reversed_tuple_bottom(&assignment.value, &self.environment());
        let incomplete_collection = self.scopes.incomplete_collection_constructor(
            &assignment.value,
            actual,
            &self.environment(),
        );
        let deferred_set = self.remember_partial_set(assignment, incomplete_collection);
        let nested_empty_list = self.nested_empty_list_assignment(assignment, actual);
        let incomplete = empty_conditional.is_some()
            || nested_empty_set.is_some()
            || nested_empty_list
            || incomplete_collection.is_some()
            || actual.is_some_and(|ty| self.context.unsolved_recursive_alias(ty))
            || (!preserve_bottom && actual.is_some_and(|ty| self.context.incomplete_iterator(ty)))
            || self
                .scopes
                .unsolved_constructor(&assignment.value, &self.environment());
        let partial_none_actual = if incomplete { None } else { actual };
        if incomplete && partial_defaultdict.is_none() {
            for target in &assignment.targets {
                if let Expr::Name(name) = target {
                    if let Some(kind) = incomplete_collection {
                        if deferred_set {
                            continue;
                        }
                        let hint = match kind {
                            types::Container::Dict => "dict[<type>, <type>]",
                            types::Container::Set => "set[<type>]",
                            _ => "list[<type>]",
                        };
                        self.emit(
                            "var-annotated",
                            format!(
                                "Need type annotation for \"{}\" (hint: \"{}: {hint} = ...\")",
                                name.id, name.id
                            ),
                            target,
                        );
                        continue;
                    }
                    self.emit(
                        "var-annotated",
                        format!("Need type annotation for \"{}\"", name.id),
                        assignment.value.as_ref(),
                    );
                }
            }
        }
        let actual = partial_defaultdict
            .as_ref()
            .map(|seed| seed.initial)
            .or_else(|| {
                self.scopes
                    .recover_constructor_assignment(&assignment.value, &self.environment())
                    .or(actual)
            })
            .map(|ty| {
                if let Some(recovered) = nested_empty_set {
                    recovered
                } else if incomplete_collection.is_some() || empty_conditional.is_some() {
                    self.context.substitute(
                        ty,
                        &BTreeMap::from([(types::Type::Never, types::Type::Any)]),
                    )
                } else {
                    self.recover_nested_empty_list(ty, nested_empty_list)
                }
            })
            .map(|ty| self.context.iteration_assignment(ty, preserve_bottom))
            .map(|ty| self.context.recover_unsolved_alias(ty));
        for target in &assignment.targets {
            if self
                .scopes
                .resolve_partial_none(target, partial_none_actual)
                .is_some()
            {
                self.refuse_local_none_inputs();
            }
            if partial_none_actual.is_some() {
                self.resolve_literal_tuple_partial_none(target, &assignment.value);
            }
        }
        if self.check_chained_index_assignment(assignment, actual) {
            return;
        }
        if self.check_chained_name_assignment(assignment, actual) {
            return;
        }
        for target in &assignment.targets {
            if let Expr::Attribute(attribute) = target {
                if self
                    .infer(&attribute.value)
                    .and_then(|ty| self.context.class_field(ty, attribute.attr.as_str()))
                    .is_some_and(|field| field.needs_annotation)
                {
                    self.emit(
                        "var-annotated",
                        format!("Need type annotation for \"{}\"", attribute.attr),
                        assignment.value.as_ref(),
                    );
                }
            }
            if self.check_typed_dict_assignment(target, &assignment.value) {
                continue;
            }
            if self.check_dictionary_assignment(target, &assignment.value) {
                continue;
            }
            if let Expr::Attribute(attribute) = target {
                if let Expr::Name(variable) = attribute.value.as_ref() {
                    if let Some(model) = self.model_instance(variable.id.as_str()) {
                        if let Some(field) = model
                            .fields
                            .iter()
                            .find(|field| field.name == attribute.attr.as_str())
                        {
                            if model.frozen || field.frozen {
                                self.emit(
                                    "misc",
                                    format!(
                                        "Property \"{}\" defined in \"{}\" is read-only",
                                        field.name, model.name
                                    ),
                                    target,
                                );
                            }
                        }
                    }
                }
            }
            if self.check_descriptor_assignment(target, &assignment.value) {
                continue;
            }
            if matches!(target, Expr::Tuple(_) | Expr::List(_)) {
                self.check_sequence_assignment(
                    target,
                    &assignment.value,
                    actual,
                    assignment.targets.len() == 1,
                );
                continue;
            }
            let ordinary = ordinary_actual
                .filter(|&ty| {
                    ty != types::Type::Never && !incomplete && partial_defaultdict.is_none()
                })
                .and_then(|_| self.ordinary_try_assignment(target));
            let mut compatibility = None;
            let actual = if let Some(expected) = self
                .scopes
                .assigned_type(target)
                .or_else(|| self.assigned_object_type(target))
                .filter(|_| partial_defaultdict.is_none())
            {
                let actual = self.infer_expected(&assignment.value, expected);
                if let Some(actual) = actual {
                    compatibility = Some(self.check_assignment_type(
                        expected,
                        actual,
                        assignment.value.as_ref(),
                    ));
                }
                actual
            } else {
                actual
            };
            self.prepare_try_assignment(target, compatibility);
            self.scopes.bind(target, actual);
            let mut declaration = deferred_set
                .then_some(try_inputs::PartialFamily::Set)
                .or(self.remember_partial_dict(assignment))
                .or(self.remember_partial_list(target, &assignment.value));
            self.flow_assignment(target, actual);
            if let Some(seed) = partial_defaultdict.as_ref() {
                if self.remember_partial_defaultdict(assignment, seed) {
                    declaration = Some(try_inputs::PartialFamily::Defaultdict);
                } else if let Expr::Name(name) = target {
                    self.emit(
                        "var-annotated",
                        format!("Need type annotation for \"{}\"", name.id),
                        assignment.value.as_ref(),
                    );
                }
            }
            if self.scopes.inferred_local_any_assignment(target) {
                if let (Expr::Name(name), Some(actual)) = (
                    target,
                    actual.filter(|&ty| {
                        !matches!(
                            ty,
                            types::Type::Any | types::Type::AnySpecial | types::Type::Never
                        )
                    }),
                ) {
                    // Keep the declaration baseline; only this reached assignment narrows it.
                    self.flow.narrowed.insert(name.id.to_string(), actual);
                }
            }
            self.scopes.remember_callable(target, &assignment.value);
            self.remember(target, &assignment.value);
            self.record_ordinary_try_commit(target, actual, ordinary, compatibility);
            self.record_partial_initializer_commit(
                target,
                actual,
                compatibility != Some(false),
                declaration,
            );
        }
        self.record_constructor_none_assignment(assignment, ordinary_actual);
    }

    fn check_chained_index_assignment(
        &mut self,
        assignment: &StmtAssign,
        actual: Option<types::Type>,
    ) -> bool {
        let Some((target, earlier)) = assignment.targets.split_last() else {
            return false;
        };
        let Some(actual) = actual.filter(|_| !earlier.is_empty()) else {
            return false;
        };
        let Expr::Subscript(subscript) = target else {
            return false;
        };
        if !earlier.iter().all(|target| matches!(target, Expr::Name(_))) {
            return false;
        }
        let Expr::Name(base) = subscript.value.as_ref() else {
            return false;
        };
        if self.flow.deleted.contains(base.id.as_str()) {
            return false;
        }
        let Some(receiver) = self
            .flow
            .narrowed
            .get(base.id.as_str())
            .copied()
            .or_else(|| self.scopes.environment().get(base.id.as_str()).copied())
        else {
            return false;
        };
        // Object has no __setitem__; check the live receiver before earlier Name writes.
        if receiver != types::Type::Object {
            return false;
        }
        let memory = self.assignment_memory(&assignment.value);
        self.emit(
            "index",
            format!(
                "Unsupported target for indexed assignment (\"{}\")",
                self.context.name(receiver)
            ),
            target,
        );
        // The remaining targets see one fixed RHS type, like mypy's TempNode.
        for target in earlier {
            let expected = self.scopes.assigned_type(target);
            if let Some(expected) = expected {
                self.check_chained_assignment_type(expected, actual, assignment.start());
            }
            let compatible = expected.is_none_or(|expected| self.context.accepts(expected, actual));
            self.scopes.bind(target, Some(actual));
            if compatible {
                self.flow_assignment(target, Some(actual));
            }
            self.remember_assignment_memory(
                target,
                AssignmentMemory {
                    callable: memory.callable.clone(),
                    model: memory.model.clone(),
                },
                compatible,
            );
            self.record_try_input_commit(target, Some(actual), compatible);
        }
        true
    }

    fn check_chained_name_assignment(
        &mut self,
        assignment: &StmtAssign,
        actual: Option<types::Type>,
    ) -> bool {
        let Some((rightmost, earlier)) = assignment.targets.split_last() else {
            return false;
        };
        let Some(actual) = actual.filter(|_| !earlier.is_empty()) else {
            return false;
        };
        if !self.chained_rhs_supported(&assignment.value)
            || !self.chained_type_supported(actual)
            || assignment.targets.iter().any(|target| {
                self.scopes
                    .assignment_bindings(target, actual)
                    .is_none_or(|bindings| {
                        bindings.iter().any(|(target, _)| {
                            self.scopes
                                .assigned_type(target)
                                .is_some_and(|ty| !self.chained_type_supported(ty))
                        })
                    })
            })
            || (!matches!(rightmost, Expr::Name(_))
                && !matches!(
                    assignment.value.as_ref(),
                    Expr::Tuple(_) | Expr::List(_) | Expr::Name(_)
                ))
        {
            return false;
        }
        let scopes = self.scopes.clone();
        let flow = self.flow.clone();
        let instances = self.instances.clone();
        let diagnostics = self.diagnostics.clone();
        let partial_sets = self.partial_sets.clone();
        let partial_list_unknown = self.partial_list_unknown.clone();
        let partial_list_gaps = self.partial_list_gaps.clone();
        let settled_diagnostics = self.settled_diagnostics.clone();
        let directives = self.directives.clone();
        let expected_calls = self.expected_calls.clone();
        let checkpoint = self.try_inputs.checkpoint();
        if self.apply_chained_name_assignment(assignment, rightmost, earlier, actual) {
            self.try_inputs.commit(checkpoint);
            return true;
        }
        self.try_inputs.rollback(checkpoint);
        self.scopes = scopes;
        self.flow = flow;
        self.instances = instances;
        self.diagnostics = diagnostics;
        self.partial_sets = partial_sets;
        self.partial_list_unknown = partial_list_unknown;
        self.partial_list_gaps = partial_list_gaps;
        self.settled_diagnostics = settled_diagnostics;
        self.directives = directives;
        self.expected_calls = expected_calls;
        false
    }

    fn chained_rhs_supported(&self, expression: &Expr) -> bool {
        match expression {
            Expr::Name(name) => {
                self.environment()
                    .get(name.id.as_str())
                    .is_some_and(|&ty| !ty.is_any() && self.chained_type_supported(ty))
                    && self.scopes.callable(name.id.as_str()).is_none()
                    && !self.models.contains_key(name.id.as_str())
                    && self
                        .scopes
                        .variables()
                        .get(name.id.as_str())
                        .is_none_or(|&ty| self.chained_type_supported(ty))
            }
            Expr::StringLiteral(_)
            | Expr::BytesLiteral(_)
            | Expr::BooleanLiteral(_)
            | Expr::NoneLiteral(_)
            | Expr::NumberLiteral(_) => true,
            Expr::Tuple(tuple) => tuple
                .elts
                .iter()
                .all(|item| self.chained_rhs_supported(item)),
            Expr::List(list) => list
                .elts
                .iter()
                .all(|item| self.chained_rhs_supported(item)),
            _ => false,
        }
    }

    fn chained_type_supported(&self, ty: types::Type) -> bool {
        if !matches!(ty, types::Type::Node(_)) {
            return true;
        }
        let Some(node) = self.context.node(ty) else {
            return false;
        };
        match node.as_ref() {
            types::Node::Container(types::Container::List, arguments) => {
                arguments.len() == 1 && self.chained_type_supported(arguments[0])
            }
            types::Node::Tuple(arguments) | types::Node::Union(arguments) => {
                arguments.iter().all(|&ty| self.chained_type_supported(ty))
            }
            types::Node::VariadicTuple(item) => self.chained_type_supported(*item),
            types::Node::Literal(_) | types::Node::LiteralValue(_) => true,
            _ => false,
        }
    }

    fn apply_chained_name_assignment(
        &mut self,
        assignment: &StmtAssign,
        rightmost: &Expr,
        earlier: &[Expr],
        actual: types::Type,
    ) -> bool {
        let memory = AssignmentMemory {
            callable: None,
            model: None,
        };
        let actual = if matches!(rightmost, Expr::Name(_)) {
            let expected = self.scopes.assigned_type(rightmost);
            let actual = if let Some(expected) = expected {
                let actual = self.infer_expected(&assignment.value, expected);
                if let Some(actual) = actual {
                    self.check_assignment_type(expected, actual, assignment.value.as_ref());
                }
                actual
            } else {
                Some(actual)
            };
            let compatible = actual.is_some_and(|actual| {
                expected.is_none_or(|expected| self.context.accepts(expected, actual))
            });
            self.scopes.bind(rightmost, actual);
            let _ = self.remember_partial_list(rightmost, &assignment.value);
            if compatible {
                self.flow_assignment(rightmost, actual);
            }
            self.remember_assignment_memory(
                rightmost,
                AssignmentMemory {
                    callable: memory.callable.clone(),
                    model: memory.model.clone(),
                },
                compatible,
            );
            self.record_try_input_commit(rightmost, actual, compatible);
            actual
        } else {
            if matches!(assignment.value.as_ref(), Expr::Tuple(_) | Expr::List(_)) {
                let Some(bindings) =
                    self.sequence_assignment_bindings(rightmost, &assignment.value, Some(actual))
                else {
                    return false;
                };
                if !bindings.iter().all(|binding| binding.expression.is_some()) {
                    return false;
                }
                self.check_literal_sequence_assignment(bindings, false, false);
            } else {
                self.check_sequence_assignment(rightmost, &assignment.value, Some(actual), false);
            }
            if matches!(assignment.value.as_ref(), Expr::Tuple(_) | Expr::List(_)) {
                self.infer(&assignment.value)
            } else {
                Some(actual)
            }
        };
        let Some(actual) = actual else {
            return false;
        };
        if !self.chained_type_supported(actual) {
            return false;
        }
        let literal_leaves: BTreeMap<_, _> = earlier
            .iter()
            .filter(|target| !matches!(target, Expr::Name(_)))
            .filter_map(|target| {
                self.sequence_assignment_bindings(target, &assignment.value, Some(actual))
            })
            .flatten()
            .filter_map(|binding| {
                binding
                    .expression
                    .map(|expression| (binding.target.start(), expression))
            })
            .collect();
        let Some(bindings): Option<Vec<_>> = earlier
            .iter()
            .map(|target| {
                self.scopes
                    .assignment_bindings(target, actual)
                    .map(|bindings| (matches!(target, Expr::Name(_)), bindings))
            })
            .collect()
        else {
            return false;
        };
        for (remember_model, bindings) in bindings {
            for (target, actual) in bindings {
                let expression = if remember_model {
                    Some(assignment.value.as_ref())
                } else {
                    literal_leaves.get(&target.start()).copied()
                };
                if let Some(expression) = expression {
                    self.prepare_partial_list_assignment(target, expression, Some(actual));
                }
                let expected = self.scopes.assigned_type(target);
                if let Some(expected) = expected {
                    self.check_chained_assignment_type(expected, actual, assignment.start());
                }
                let compatible =
                    expected.is_none_or(|expected| self.context.accepts(expected, actual));
                self.scopes.bind(target, Some(actual));
                if let Some(expression) = expression {
                    let _ = self.remember_partial_list(target, expression);
                }
                if compatible {
                    self.flow_assignment(target, Some(actual));
                }
                if remember_model {
                    self.remember_assignment_memory(
                        target,
                        AssignmentMemory {
                            callable: None,
                            model: memory.model.clone(),
                        },
                        compatible,
                    );
                } else if compatible {
                    self.scopes.remember_assigned_callable(target, None);
                }
                self.record_try_input_commit(target, Some(actual), compatible);
            }
        }
        true
    }

    fn check_sequence_assignment(
        &mut self,
        target: &Expr,
        value: &Expr,
        actual: Option<types::Type>,
        register_partial_sets: bool,
    ) {
        self.check_fixed_tuple_unpack_count(target, actual, value);
        let Some(bindings) = self.sequence_assignment_bindings(target, value, actual) else {
            return;
        };
        if matches!(value, Expr::Tuple(_) | Expr::List(_))
            && bindings.iter().all(|binding| {
                matches!(binding.target, Expr::Name(_)) && binding.expression.is_some()
            })
        {
            // Chained/shared RHS needs its existing fixed-value policy. Also
            // decline duplicate targets rather than guess a new alias lifecycle.
            let mut names = BTreeSet::new();
            let register_partial_sets = register_partial_sets
                && bindings.iter().all(|binding| {
                    matches!(binding.target, Expr::Name(name) if names.insert(name.id.as_str()))
                });
            self.check_literal_sequence_assignment(bindings, true, register_partial_sets);
            return;
        }
        let mut prepared = Vec::with_capacity(bindings.len());
        let mut inferred_declarations = BTreeMap::new();
        for binding in bindings {
            let target = binding.target;
            let expected = self.scopes.assigned_type(target).or_else(|| {
                let Expr::Name(name) = target else {
                    return None;
                };
                inferred_declarations.get(name.id.as_str()).copied()
            });
            let mut compatible = expected.is_none();
            let actual = if let Some(expected) = expected {
                let actual = if let Some(expression) = binding.expression {
                    self.infer_expected(expression, expected)
                } else {
                    Some(binding.ty)
                };
                if let Some(actual) = actual {
                    compatible = if let Some(expression) = binding.expression {
                        self.check_assignment_type(expected, actual, expression)
                    } else {
                        self.check_chained_assignment_type(expected, actual, value.start())
                    };
                }
                actual
            } else {
                Some(binding.ty)
            };
            if let (None, Expr::Name(name), Some(actual)) = (expected, target, actual) {
                inferred_declarations.insert(name.id.to_string(), actual);
            }
            let memory = binding
                .expression
                .map(|expression| self.assignment_memory(expression));
            prepared.push(PreparedAssignment {
                target,
                actual,
                memory,
                compatible,
            });
        }
        for assignment in prepared {
            self.scopes.bind(assignment.target, assignment.actual);
            self.flow_assignment(assignment.target, assignment.actual);
            if let Some(memory) = assignment.memory {
                self.remember_assignment_memory(assignment.target, memory, true);
            }
            self.record_try_input_commit(
                assignment.target,
                assignment.actual,
                assignment.compatible,
            );
        }
    }

    fn check_literal_sequence_assignment(
        &mut self,
        bindings: Vec<AssignmentBinding<'_>>,
        capture_callable: bool,
        register_partial_sets: bool,
    ) {
        for binding in bindings {
            let target = binding.target;
            let expression = binding.expression.expect("literal assignment leaf");
            let initial = self.partial_list_observed_type(expression);
            self.prepare_partial_list_assignment(target, expression, initial);
            let expected = self.scopes.assigned_type(target);
            let actual = if let Some(expected) = expected {
                let actual = self.infer_expected(expression, expected);
                if let Some(actual) = actual {
                    self.check_assignment_type(expected, actual, expression);
                }
                actual
            } else {
                self.infer(expression)
            };
            let (actual, declaration) = if register_partial_sets {
                self.remember_literal_partial_set(target, expression, actual, expected.is_none())
            } else {
                (actual, None)
            };
            let compatible = actual.is_some_and(|actual| {
                expected.is_none_or(|expected| self.context.accepts(expected, actual))
            });
            let memory = if capture_callable {
                self.assignment_memory(expression)
            } else {
                AssignmentMemory {
                    callable: None,
                    model: None,
                }
            };
            self.scopes.bind(target, actual);
            let declaration = declaration.or(self.remember_partial_list(target, expression));
            if compatible {
                self.flow_assignment(target, actual);
            }
            self.remember_assignment_memory(target, memory, compatible);
            self.record_partial_initializer_commit(target, actual, compatible, declaration);
        }
    }

    fn assignment_memory(&self, value: &Expr) -> AssignmentMemory {
        let model = match value {
            Expr::Call(call) => match call.func.as_ref() {
                Expr::Name(name) => Some(name.id.as_str()),
                _ => None,
            },
            Expr::Name(name) => Some(name.id.as_str()),
            _ => None,
        };
        AssignmentMemory {
            callable: self.scopes.assignment_callable(value),
            model: model
                .filter(|model| self.models.contains_key(*model))
                .map(str::to_owned),
        }
    }

    fn remember_assignment_memory(
        &mut self,
        target: &Expr,
        memory: AssignmentMemory,
        remember_callable: bool,
    ) {
        if remember_callable {
            self.scopes
                .remember_assigned_callable(target, memory.callable);
        }
        if let Expr::Name(variable) = target {
            let current = self.instances.last_mut().expect("module instance scope");
            if let Some(model) = memory.model {
                current.insert(variable.id.to_string(), model);
            } else {
                current.remove(variable.id.as_str());
            }
        }
    }

    pub(super) fn check_assignment_type(
        &mut self,
        expected: types::Type,
        actual: types::Type,
        value: &impl Ranged,
    ) -> bool {
        let accepted = self.context.accepts(expected, actual);
        if !accepted {
            let hint = self.context.variance_hint(expected, actual);
            let paired = self.context.assignment_callable_names(actual, expected);
            let actual = if let Some((actual, _)) = &paired {
                format!("\"{actual}\"")
            } else if !self.context.is_class_value(actual)
                && self
                    .context
                    .callable_signature(actual)
                    .is_some_and(|signature| !signature.overloads.is_empty())
            {
                "overloaded function".into()
            } else {
                format!("\"{}\"", self.context.name(actual))
            };
            let expected = paired.map_or_else(
                || self.context.assignment_type_name(expected),
                |(_, expected)| expected,
            );
            self.emit_with_hint("assignment", format!("Incompatible types in assignment (expression has type {actual}, variable has type \"{expected}\")"), value, hint);
        }
        accepted
    }
}

#[cfg(test)]
mod tuple_binding_recovery_tests {
    use crate::domain::{ports::ProjectStore, source::PythonVersion};
    use crate::infrastructure::filesystem::Filesystem;

    #[test]
    fn unknown_call_unpacking_binds_targets_without_hiding_undefined_names() {
        let source = r#"from typing import Any

def unpack(spec: str, dynamic: Any, flag: bool) -> None:
    name, sep, value = spec.partition("=")
    module, names = spec.split(":", 1)
    dep_path, _, dep_name = dynamic.rpartition(":") if flag else (spec, "", dynamic)
    print(name, sep, value, module, names, dep_path, dep_name)
    print(unbound_sentinel)
"#;
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("main.py");
        std::fs::write(&path, source).unwrap();
        let mut workspace = Filesystem.load(std::slice::from_ref(&path), true);
        workspace.projects[0].python_version = PythonVersion::Py312;
        let report = crate::infrastructure::mypy::check(&workspace);
        let undefined: Vec<_> = report
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.rule == "name-defined")
            .map(|diagnostic| diagnostic.message.as_str())
            .collect();
        assert_eq!(undefined, ["Name \"unbound_sentinel\" is not defined"]);
        assert_eq!(std::fs::read(&path).unwrap(), source.as_bytes());
    }
}
