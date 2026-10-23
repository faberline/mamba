//! Container literals receive their expected element type before assignment checks.
use super::*;
mod comprehensions;
use types::{Container, Node, Type};

impl Checker<'_> {
    pub(super) fn infer_expected(&mut self, expression: &Expr, expected: Type) -> Option<Type> {
        let pointer = expression as *const Expr as usize;
        if self.checked_expression_owner == Some(self.scopes.try_input_scope())
            && !self.context.has_observed_expression_types()
            && self.checked_expression_contexts.get(&pointer) == Some(&Some(expected))
        {
            if let Some(values) = &self.checked_expression_types {
                if let Some(actual) = values
                    .lock()
                    .expect("owned checked expressions")
                    .get(&pointer)
                    .copied()
                {
                    return actual;
                }
            }
        }
        let actual = self.infer_expected_inner(expression, expected);
        if actual.is_none() {
            self.typed_dict_trial_failed = self.typed_dict_trial_failed.map(|_| true);
        }
        actual
    }

    pub(super) fn dictionary_comprehension_context(&self, expected: Type) -> Type {
        if self.context.class_id(expected).is_some_and(|id| {
            self.context.class(id).namespace == "_typeshed.SupportsKeysAndGetItem"
        }) {
            let arguments = self.context.class_arguments(expected);
            if arguments.len() == 2 {
                return self
                    .context
                    .intern(Node::Container(Container::Dict, arguments));
            }
        }
        expected
    }

    fn infer_expected_inner(&mut self, expression: &Expr, expected: Type) -> Option<Type> {
        let expected = self.context.expanded_alias_instance(expected);
        if let Some(actual) = self.infer_lambda_expected(expression, expected) {
            return actual;
        }
        if let Some(actual) = self.infer_generator_expected(expression, expected) {
            return Some(actual);
        }
        let comprehension_expected = if matches!(expression, Expr::DictComp(_)) {
            self.dictionary_comprehension_context(expected)
        } else {
            expected
        };
        if let Some(actual) = self.infer_comprehension_expected(expression, comprehension_expected)
        {
            return actual;
        }
        if let Expr::Dict(_) = expression {
            let actual = self
                .context
                .infer_with_calls(expression, &self.environment(), &|value| {
                    self.scopes.expression_type(value, &self.environment())
                });
            if let Some(contextual) =
                actual.and_then(|actual| self.context.reversed_literal_context(expected, actual))
            {
                return self.infer_expected(expression, contextual);
            }
            if self.context.class_id(expected).is_some_and(|id| {
                self.context.class(id).namespace == "_typeshed.SupportsKeysAndGetItem"
            }) {
                let expected = self.context.intern(Node::Container(
                    Container::Dict,
                    self.context.class_arguments(expected),
                ));
                return self.infer_expected(expression, expected);
            }
        }
        if let Expr::Set(set) = expression {
            if let Some(item) = self.context.abstract_set_item(expected) {
                self.check_items(&set.elts, item, "Set", "arg-type");
                return Some(
                    self.context
                        .intern(Node::Container(Container::Set, vec![item])),
                );
            }
        }
        if matches!(expression, Expr::List(_) | Expr::Set(_) | Expr::Dict(_)) {
            let kind = match expression {
                Expr::List(_) => Some(Container::List),
                Expr::Set(_) => Some(Container::Set),
                _ => None,
            };
            let actual = kind
                .map(|kind| self.context.intern(Node::Container(kind, vec![Type::Any])))
                .unwrap_or_else(|| {
                    self.context
                        .intern(Node::Container(Container::Dict, vec![Type::Any; 2]))
                });
            if self.context.class_id(expected).is_some_and(|id| {
                self.context
                    .builtin_collection_arguments(actual, id)
                    .is_some()
                    || matches!(
                        self.context.class(id).namespace.as_str(),
                        "_typeshed.SupportsIter" | "builtins._SupportsReversed"
                    )
            }) {
                if let Some(item) = self.context.collection_literal_item(expected) {
                    let contextual = self
                        .context
                        .intern(Node::Container(Container::Iterable, vec![item]));
                    return self.infer_expected(expression, contextual);
                }
            }
        }
        if let Expr::Await(awaited) = expression {
            let operand_expected = self
                .context
                .intern(Node::Container(Container::Awaitable, vec![expected]));
            let operand = self.infer_expected(&awaited.value, operand_expected)?;
            return Some(self.context.await_result(operand).unwrap_or(Type::Any));
        }
        if let Expr::Dict(dictionary) = expression {
            if self.context.typed_dict(expected).is_some() {
                return self.check_typed_dict_literal(dictionary, expected, dictionary);
            }
            if let Some(actual) = self.infer_typed_dict_union(dictionary, expected) {
                return Some(actual);
            }
            if dictionary.items.is_empty()
                && !expected.is_any()
                && self.context.mapping_types(expected).is_none()
                && !matches!(self.context.node(expected).as_deref(), Some(Node::Union(_)))
            {
                return Some(self.context.intern(Node::Container(
                    Container::Dict,
                    vec![Type::Never, Type::Never],
                )));
            }
        }
        if self.context.contains_literal(expected) {
            if let Some(ty) = self.context.literal_expression(expression) {
                return Some(ty);
            }
        }
        if let Expr::Call(call) = expression {
            self.expected_calls
                .insert((call.start(), call.end()), expected);
            if let Some(signature) = self
                .scopes
                .callable_expression_in(&call.func, &self.environment())
            {
                if let Some(dictionary) = self.scopes.dictionary_expression(call, &signature) {
                    return self.infer_expected(&dictionary, expected);
                }
                if let Some(variant) = signature.overloads.first() {
                    let solution = self.scopes.call_solution(
                        call,
                        variant,
                        &self.environment(),
                        Some(expected),
                    );
                    if let (Some(parameter), Some(value)) =
                        (variant.parameters.first(), call.arguments.args.first())
                    {
                        let parameter = parameter
                            .ty
                            .map(|ty| self.context.substitute(ty, &solution.types));
                        if parameter.is_some_and(|ty| {
                            self.context.class_id(ty).is_some_and(|id| {
                                matches!(
                                    self.context.class(id).namespace.as_str(),
                                    "_typeshed.SupportsIter" | "builtins._SupportsReversed"
                                )
                            })
                        }) && parameter
                            .is_some_and(|ty| self.context.collection_literal_item(ty).is_some())
                            && matches!(value, Expr::List(_) | Expr::Set(_) | Expr::Dict(_))
                            && (signature.name.as_deref() != Some("reversed")
                                || !matches!(value, Expr::Set(_)))
                        {
                            if let Some(returns) = variant.returns {
                                return Some(self.context.substitute(returns, &solution.types));
                            }
                        }
                    }
                }
            }
            if let Some(ty) =
                self.scopes
                    .call_return_context(call, &self.environment(), Some(expected))
            {
                return Some(ty);
            }
        }
        let Some(node) = self.context.node(expected) else {
            return self.infer(expression);
        };
        match (expression, node.as_ref()) {
            (_, Node::Union(items)) => {
                let branches: Vec<_> = items
                    .iter()
                    .copied()
                    .filter(|&item| {
                        self.context
                            .node(self.context.expanded_alias_instance(item))
                            .is_some_and(|node| {
                                matches!(
                                    (expression, node.as_ref()),
                                    (
                                        Expr::List(_),
                                        Node::Container(
                                            Container::List
                                                | Container::Sequence
                                                | Container::Iterable,
                                            _
                                        )
                                    ) | (
                                        Expr::Set(_),
                                        Node::Container(Container::Set | Container::Iterable, _)
                                    ) | (
                                        Expr::Dict(_),
                                        Node::Container(
                                            Container::Dict
                                                | Container::Mapping
                                                | Container::Iterable,
                                            _
                                        )
                                    ) | (Expr::Tuple(_), Node::Tuple(_) | Node::VariadicTuple(_))
                                )
                            })
                    })
                    .collect();
                let actual = self.infer(expression);
                let branch = branches
                    .iter()
                    .copied()
                    .find(|&item| actual.is_some_and(|actual| self.context.accepts(item, actual)))
                    .or_else(|| branches.first().copied());
                if let Some(expected) = branch {
                    self.infer_expected(expression, expected)
                } else {
                    if matches!(expression, Expr::List(list) if list.elts.is_empty()) {
                        Some(
                            self.context
                                .intern(Node::Container(Container::List, vec![Type::Never])),
                        )
                    } else if matches!(expression, Expr::Dict(dictionary) if dictionary.items.is_empty())
                    {
                        Some(self.context.intern(Node::Container(
                            Container::Dict,
                            vec![Type::Never, Type::Never],
                        )))
                    } else {
                        self.infer(expression)
                    }
                }
            }
            (Expr::List(list), Node::Container(Container::List, args)) => {
                self.check_items(&list.elts, args[0], "List", "list-item");
                Some(expected)
            }
            (Expr::List(list), Node::Container(Container::Sequence, args)) => {
                let item = self.context.simplify_inference_union(args[0]);
                self.check_items(&list.elts, item, "List", "list-item");
                Some(
                    self.context
                        .intern(Node::Container(Container::List, vec![item])),
                )
            }
            (Expr::List(list), Node::Container(Container::Iterable, args)) => {
                self.check_items(&list.elts, args[0], "List", "list-item");
                Some(
                    self.context
                        .intern(Node::Container(Container::List, vec![args[0]])),
                )
            }
            (Expr::Set(set), Node::Container(Container::Iterable, args)) => {
                self.check_items(&set.elts, args[0], "Set", "arg-type");
                Some(
                    self.context
                        .intern(Node::Container(Container::Set, vec![args[0]])),
                )
            }
            (Expr::Dict(_), Node::Container(Container::Iterable, args)) => {
                let actual = self.infer(expression)?;
                let values = self.context.mapping_types(actual)?.1;
                let expected = self
                    .context
                    .intern(Node::Container(Container::Dict, vec![args[0], values]));
                self.infer_expected(expression, expected)
            }
            (Expr::Dict(_), Node::Container(Container::Mapping, args)) => {
                let expected = self
                    .context
                    .intern(Node::Container(Container::Dict, args.clone()));
                self.infer_expected(expression, expected)
            }
            (Expr::Set(set), Node::Container(Container::Set, args)) => {
                self.check_items(&set.elts, args[0], "Set", "arg-type");
                Some(expected)
            }
            (Expr::List(list), Node::Container(Container::Dict, args))
                if list.elts.is_empty()
                    && args.len() == 2
                    && self.context.observed_expression_type(expression).is_none()
                    && self.observed_fstring_type(expression).is_none() =>
            {
                Some(
                    self.context
                        .intern(Node::Container(Container::List, vec![Type::Never])),
                )
            }
            (Expr::Dict(dictionary), Node::Container(Container::Dict, args)) => {
                self.check_dictionary_items(dictionary, args[0], args[1]);
                Some(expected)
            }
            (Expr::Tuple(tuple), Node::Tuple(args)) if tuple.elts.len() == args.len() => {
                let mut items = Vec::new();
                for (item, &expected) in tuple.elts.iter().zip(args) {
                    items.push(self.infer_expected(item, expected)?);
                }
                Some(self.context.intern(Node::Tuple(items)))
            }
            (Expr::Tuple(tuple), Node::VariadicTuple(item)) => {
                let mut items = Vec::new();
                for expression in &tuple.elts {
                    items.push(self.infer_expected(expression, *item)?);
                }
                Some(self.context.intern(Node::Tuple(items)))
            }
            _ => self.infer(expression),
        }
    }

    pub(super) fn check_dictionary_index(
        &mut self,
        subscript: &quality_syntax::python_ast::ExprSubscript,
    ) {
        let Some(receiver) = self.infer(&subscript.value) else {
            return;
        };
        let receiver = self.context.expanded_alias_instance(receiver);
        let Some(node) = self.context.node(receiver) else {
            return;
        };
        let Node::Container(Container::Dict, arguments) = node.as_ref() else {
            return;
        };
        let Some(actual) = self.infer(&subscript.slice) else {
            return;
        };
        self.check_dictionary_index_types(subscript, receiver, arguments[0], actual);
    }

    fn check_dictionary_index_types(
        &mut self,
        subscript: &quality_syntax::python_ast::ExprSubscript,
        receiver: Type,
        expected: Type,
        actual: Type,
    ) -> bool {
        if !self.context.accepts(expected, actual) {
            if subscript.ctx.is_del() {
                self.emit(
                    "arg-type",
                    format!(
                        "Argument 1 to \"__delitem__\" of \"dict\" has incompatible type \"{}\"; expected \"{}\"",
                        self.context.name(actual),
                        self.context.name(expected)
                    ),
                    subscript.slice.as_ref(),
                );
                return false;
            }
            self.emit(
                "index",
                format!(
                    "Invalid index type \"{}\" for \"{}\"; expected type \"{}\"",
                    self.context.name(actual),
                    self.context.name(receiver),
                    self.context.name(expected)
                ),
                subscript.slice.as_ref(),
            );
            return false;
        }
        true
    }

    pub(super) fn check_candidate_subscript(
        &mut self,
        subscript: &quality_syntax::python_ast::ExprSubscript,
        receiver: super::generator_inputs::CheckedValue,
    ) -> super::generator_inputs::CheckedExpression {
        use super::generator_inputs::{unknown_expression, CandidateFailure, CheckedValue};
        let receiver_type = receiver.actual();
        let mut checked_values = vec![receiver];
        // Even a recovery/dynamic receiver checks the actual index children.
        if let Expr::Slice(slice) = subscript.slice.as_ref() {
            for value in [&slice.lower, &slice.upper, &slice.step]
                .into_iter()
                .flatten()
            {
                let actual = self.check_candidate_expression(value, None)?;
                if !matches!(
                    actual.actual(),
                    Type::Int | Type::Bool | Type::None | Type::Any
                ) {
                    return unknown_expression(value.as_ref(), "unproved slice component");
                }
                checked_values.push(actual);
            }
            if receiver_type == Type::Any {
                return Ok(CheckedValue::combine(Type::Any, checked_values));
            }
            if matches!(self.context.node(receiver_type).as_deref(),
                Some(Node::Container(Container::List, items)) if items.len() == 1)
            {
                return Ok(CheckedValue::combine(receiver_type, checked_values));
            }
            return unknown_expression(subscript, "unproved slicing receiver");
        }
        let node = self.context.node(receiver_type);
        let expected_index = node.as_deref().and_then(|node| match node {
            Node::Container(Container::Dict, arguments) if arguments.len() == 2 => {
                Some(arguments[0])
            }
            _ => None,
        });
        let index = self.check_candidate_expression(&subscript.slice, expected_index)?;
        let index_type = index.actual();
        checked_values.push(index);
        if receiver_type == Type::Any {
            return Ok(CheckedValue::combine(Type::Any, checked_values));
        }
        let Some(node) = node else {
            return unknown_expression(subscript, "unproved subscript receiver");
        };
        let actual = match node.as_ref() {
            Node::Container(Container::Dict, arguments) if arguments.len() == 2 => {
                if !self.check_dictionary_index_types(
                    subscript,
                    receiver_type,
                    arguments[0],
                    index_type,
                ) {
                    return Err(CandidateFailure::Rejected);
                }
                arguments[1]
            }
            Node::Container(Container::List | Container::Sequence, arguments)
                if arguments.len() == 1
                    && matches!(index_type, Type::Int | Type::Bool | Type::Any) =>
            {
                arguments[0]
            }
            Node::Tuple(items) => {
                let Expr::NumberLiteral(number) = subscript.slice.as_ref() else {
                    return unknown_expression(subscript, "nonliteral fixed tuple index");
                };
                let quality_syntax::python_ast::Number::Int(index) = &number.value else {
                    return unknown_expression(subscript, "noninteger fixed tuple index");
                };
                let Some(index) = index.as_usize() else {
                    return unknown_expression(subscript, "negative fixed tuple index");
                };
                let Some(&item) = items.get(index) else {
                    self.emit("misc", "Tuple index out of range".into(), subscript);
                    return Err(CandidateFailure::Rejected);
                };
                item
            }
            _ => return unknown_expression(subscript, "subscript producer is not covered"),
        };
        Ok(CheckedValue::combine(actual, checked_values))
    }

    pub(super) fn check_dictionary_assignment(&mut self, target: &Expr, value: &Expr) -> bool {
        let Expr::Subscript(subscript) = target else {
            return false;
        };
        self.resolve_partial_dict_index(subscript, value);
        let Some(receiver) = self.infer(&subscript.value) else {
            return false;
        };
        let receiver = self.context.expanded_alias_instance(receiver);
        if receiver == Type::Object
            || self
                .context
                .canonical_collection_item(receiver, self.python_target)
                .is_some()
        {
            self.emit(
                "index",
                format!(
                    "Unsupported target for indexed assignment (\"{}\")",
                    self.context.name(receiver)
                ),
                target,
            );
            return true;
        }
        let Some(node) = self.context.node(receiver) else {
            return false;
        };
        let Node::Container(Container::Dict, arguments) = node.as_ref() else {
            return false;
        };
        let expected = arguments[1];
        if let Some(actual) = self.infer_expected(value, expected) {
            if !self.context.accepts(expected, actual) {
                self.emit_with_hint(
                    "assignment",
                    format!(
                        "Incompatible types in assignment (expression has type \"{}\", target has type \"{}\")",
                        self.context.name(actual),
                        self.context.assignment_type_name(expected)
                    ),
                    value,
                    self.context.variance_hint(expected, actual),
                );
            }
        }
        true
    }

    fn check_items(&mut self, items: &[Expr], expected: Type, label: &str, rule: &str) {
        let expected = self.context.expanded_alias_instance(expected);
        for (index, item) in items.iter().enumerate() {
            if let Some(actual) = self.infer_expected(item, expected) {
                if !self.context.accepts(expected, actual) {
                    let message = if label == "Set" {
                        format!(
                            "Argument {} to <set> has incompatible type \"{}\"; expected \"{}\"",
                            index + 1,
                            self.context.name(actual),
                            self.context.name(expected)
                        )
                    } else {
                        format!(
                            "{label} item {index} has incompatible type \"{}\"; expected \"{}\"",
                            self.context.name(actual),
                            self.context.name(expected)
                        )
                    };
                    self.emit(rule, message, item);
                }
            }
        }
    }
}
