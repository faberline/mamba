use super::*;
mod starred;

impl Checker<'_> {
    pub(in super::super) fn candidate_variadics_are_unused(
        call: &ExprCall,
        signature: &Signature,
    ) -> bool {
        if call.arguments.args.iter().any(Expr::is_starred_expr)
            || call
                .arguments
                .keywords
                .iter()
                .any(|keyword| keyword.arg.is_none())
        {
            return false;
        }
        let positional = signature
            .parameters
            .iter()
            .filter(|parameter| {
                matches!(
                    parameter.kind,
                    Kind::PositionalOnly | Kind::PositionalOrKeyword
                )
            })
            .count();
        signature
            .parameters
            .iter()
            .all(|parameter| match parameter.kind {
                Kind::VariadicPositional => {
                    !parameter.required && call.arguments.args.len() <= positional
                }
                Kind::VariadicKeyword => {
                    !parameter.required
                        && call.arguments.keywords.iter().all(|keyword| {
                            signature.parameters.iter().any(|fixed| {
                                matches!(fixed.kind, Kind::PositionalOrKeyword | Kind::KeywordOnly)
                                    && keyword
                                        .arg
                                        .as_ref()
                                        .is_some_and(|name| name.as_str() == fixed.name)
                            })
                        })
                }
                _ => true,
            })
    }

    pub(in super::super) fn bind_candidate_call(
        &mut self,
        call: &ExprCall,
        signature: &Signature,
        prepared: &[types::Type],
    ) -> super::super::generator_inputs::CheckedExpression {
        use super::super::generator_inputs::{unknown_expression, CandidateFailure, CheckedValue};
        let flat = if signature.parameters.iter().any(|parameter| {
            matches!(
                parameter.kind,
                Kind::VariadicPositional | Kind::VariadicKeyword
            )
        }) {
            // The original mapping proves arity, duplicates and required inputs.
            // Every actual must map to a fixed parameter before either empty
            // variadic slot can be omitted from this finite checked adapter.
            if self
                .scopes
                .contextual_overload_arguments(call, signature, prepared)
                .is_none()
                || !Self::candidate_variadics_are_unused(call, signature)
            {
                return unknown_expression(
                    call,
                    "consumed or unproved variadic candidate argument",
                );
            }
            Some(Signature {
                parameters: signature
                    .parameters
                    .iter()
                    .filter(|parameter| {
                        !matches!(
                            parameter.kind,
                            Kind::VariadicPositional | Kind::VariadicKeyword
                        )
                    })
                    .cloned()
                    .collect::<Vec<_>>()
                    .into(),
                ..signature.clone()
            })
        } else {
            None
        };
        let signature = flat.as_ref().unwrap_or(signature);
        let expected_return = self
            .expected_value
            .and_then(|(range, ty)| (range == call.range()).then_some(ty));
        let Some(preliminary) =
            self.scopes
                .checked_call_solution(call, signature, prepared, expected_return)
        else {
            return unknown_expression(call, "unproved flat argument mapping");
        };
        if !preliminary.unresolved.is_empty()
            || !preliminary.failed.is_empty()
            || !preliminary.invalid.is_empty()
        {
            return unknown_expression(call, "incomplete preliminary generic context");
        }
        let Some(pairs) = self
            .scopes
            .contextual_overload_arguments(call, signature, prepared)
        else {
            return unknown_expression(call, "unproved contextual argument mapping");
        };
        let mut actuals = Vec::new();
        let mut checked_values = Vec::new();
        let mut contexts = Vec::new();
        for ((formal, _), value) in pairs.iter().zip(
            call.arguments
                .args
                .iter()
                .chain(call.arguments.keywords.iter().map(|keyword| &keyword.value)),
        ) {
            let expected = self.context.substitute(*formal, &preliminary.types);
            if !self.context.candidate_value_role(expected, 0) {
                return unknown_expression(value, "unproved contextual parameter role");
            }
            let previous = self.provisional_generator_context;
            self.provisional_generator_context = !signature.generic.is_empty();
            let checked = self.check_candidate_expression(value, Some(expected));
            self.provisional_generator_context = previous;
            let actual = checked?;
            contexts.push(expected);
            actuals.push(actual.actual());
            checked_values.push(actual);
        }
        let recovered = checked_values.iter().any(CheckedValue::is_recovered);
        let Some(solution) =
            self.scopes
                .checked_call_solution(call, signature, &actuals, expected_return)
        else {
            return unknown_expression(call, "unproved checked argument solution");
        };
        if !solution.unresolved.is_empty() {
            return unknown_expression(call, "unresolved checked argument solution");
        }
        self.emit_call_type_variable_errors(call, signature, &solution);
        if !solution.failed.is_empty() || !solution.invalid.is_empty() {
            return Err(CandidateFailure::Rejected);
        }
        for (((formal, _), &context), value) in pairs.iter().zip(&contexts).zip(
            call.arguments
                .args
                .iter()
                .chain(call.arguments.keywords.iter().map(|keyword| &keyword.value)),
        ) {
            if self.context.substitute(*formal, &solution.types) != context {
                return unknown_expression(value, "changed checked argument context");
            }
        }
        for (((_, &actual), &expected), (index, value)) in
            pairs.iter().zip(&actuals).zip(&contexts).zip(
                call.arguments
                    .args
                    .iter()
                    .chain(call.arguments.keywords.iter().map(|keyword| &keyword.value))
                    .enumerate(),
            )
        {
            if self.check_deleted_argument(actual, value) {
                return Err(CandidateFailure::Rejected);
            }
            if !self.context.accepts(expected, actual) {
                let argument = if index < call.arguments.args.len() {
                    (index + 1).to_string()
                } else {
                    format!(
                        "\"{}\"",
                        call.arguments.keywords[index - call.arguments.args.len()]
                            .arg
                            .as_ref()
                            .unwrap()
                    )
                };
                let target = signature
                    .display_target()
                    .map(|name| format!(" to {name}"))
                    .unwrap_or_default();
                self.emit_with_hint(
                    "arg-type",
                    format!(
                        "Argument {argument}{target} has incompatible type \"{}\"; expected \"{}\"",
                        self.context.name(actual),
                        self.context.assignment_type_name(expected)
                    ),
                    value,
                    self.context.variance_hint(expected, actual),
                );
                // The existing normal generator producer reports its item
                // error, then supplies the contextual generator type. Keep
                // the real item here for that same selected fallback check.
                // Other argument errors have no such recovery contract.
                if !recovered
                    || !(matches!(value, Expr::Generator(_))
                        && self.context.iterable_item(expected).is_some()
                        || matches!(value, Expr::DictComp(_))
                            && matches!(self.context.node(self.dictionary_comprehension_context(expected)).as_deref(),
                            Some(types::Node::Container(types::Container::Dict, arguments)) if arguments.len() == 2))
                {
                    return Err(CandidateFailure::Rejected);
                }
            }
        }
        if self.typed_dict_trial_failed == Some(true) && !recovered {
            return Err(CandidateFailure::Rejected);
        }
        let Some(returned) = signature.returns else {
            return unknown_expression(call, "missing checked call return");
        };
        let actual = self.context.substitute(returned, &solution.types);
        if !self.context.candidate_value_role(actual, 0) {
            return unknown_expression(call, "unproved checked call return role");
        }
        Ok(CheckedValue::combine(
            self.context.iteration_return(signature, actual),
            checked_values,
        ))
    }

    pub(in super::super) fn bind_call(&mut self, call: &ExprCall, signature: &Signature) {
        let _ = self.bind_call_checked(call, signature);
    }

    pub(in super::super) fn bind_call_checked(
        &mut self,
        call: &ExprCall,
        signature: &Signature,
    ) -> bool {
        let local = signature
            .definition_module
            .as_ref()
            .filter(|module| format!("{module}.") == self.scopes.qualified_name(""))
            .map(|_| Signature {
                definition_module: None,
                ..signature.clone()
            });
        let signature = local.as_ref().unwrap_or(signature);
        let original_call = call;
        let expanded_call = self.scopes.collection_expanded_call(call, signature);
        let call = expanded_call.as_deref().unwrap_or(call);
        if self.check_enumerate_mapping(call, signature) {
            return false;
        }
        if call.arguments.args.iter().any(Expr::is_starred_expr)
            || call
                .arguments
                .keywords
                .iter()
                .any(|keyword| keyword.arg.is_none())
        {
            if call.arguments.args.iter().any(Expr::is_starred_expr) {
                self.check_starred_call_arguments(call, signature);
            } else {
                self.check_keyword_mapping_arguments(call, signature);
            }
            return false; // Other unpacked argument data needs the container-type model.
        }
        let expected_return = self
            .expected_value
            .and_then(|(range, ty)| (range == call.range()).then_some(ty))
            .or_else(|| {
                self.expected_calls
                    .get(&(call.start(), call.end()))
                    .copied()
            });
        let solution =
            self.scopes
                .call_solution(call, signature, &self.environment(), expected_return);
        let expanded = self
            .context
            .callable_needs_unpack(signature)
            .then(|| self.context.substitute_callable(signature, &solution.types));
        let template = signature;
        let signature = expanded.as_ref().unwrap_or(signature);
        let parameters = &signature.parameters;
        let positional: Vec<_> = parameters
            .iter()
            .enumerate()
            .filter(|(_, parameter)| {
                matches!(
                    parameter.kind,
                    Kind::PositionalOnly | Kind::PositionalOrKeyword
                )
            })
            .map(|(index, _)| index)
            .collect();
        let variadic = parameters
            .iter()
            .position(|parameter| parameter.kind == Kind::VariadicPositional);
        let kwargs = parameters
            .iter()
            .position(|parameter| parameter.kind == Kind::VariadicKeyword);
        let mut provided = vec![false; parameters.len()];
        let mut arguments = Vec::new();
        let mut invalid = false;
        let extra_positionals = call.arguments.args.len() > positional.len() && variadic.is_none();
        if extra_positionals {
            self.check_unmapped_positionals(call, signature, positional.len());
            let message = if parameters
                .iter()
                .any(|parameter| parameter.kind == Kind::KeywordOnly)
            {
                "Too many positional arguments"
            } else {
                "Too many arguments"
            };
            self.emit_with_hint(
                "call-arg",
                format!("{message}{}", signature.for_function()),
                call,
                (message == "Too many positional arguments")
                    .then(|| signature.definition_note())
                    .flatten(),
            );
            self.check_extra_positionals(call, signature, positional.len());
        }
        for (index, value) in call.arguments.args.iter().enumerate() {
            if let Some(parameter) = positional.get(index).copied().or(variadic) {
                provided[parameter] = true;
                arguments.push((parameter, (index + 1).to_string(), value));
            }
        }
        for keyword in &call.arguments.keywords {
            let name = keyword.arg.as_ref().unwrap().as_str();
            let parameter = parameters
                .iter()
                .position(|parameter| {
                    parameter.name == name
                        && matches!(
                            parameter.kind,
                            Kind::PositionalOrKeyword | Kind::KeywordOnly
                        )
                })
                .or(kwargs);
            let Some(parameter) = parameter else {
                self.emit_with_hint(
                    "call-arg",
                    format!(
                        "Unexpected keyword argument \"{name}\"{}",
                        signature.for_function()
                    ),
                    call,
                    signature.definition_note(),
                );
                invalid = true;
                continue;
            };
            if provided[parameter] && parameters[parameter].kind != Kind::VariadicKeyword {
                self.emit(
                    "misc",
                    format!(
                        "{} gets multiple values for keyword argument \"{name}\"",
                        signature
                            .display_target()
                            .unwrap_or_else(|| "Function".into())
                    ),
                    call,
                );
                invalid = true;
            }
            provided[parameter] = true;
            arguments.push((parameter, format!("\"{name}\""), &keyword.value));
        }
        self.check_missing_call_arguments(call, signature, &provided, invalid, extra_positionals);
        self.emit_call_type_variable_errors(call, signature, &solution);
        // This receipt observes the normal binder. It never repeats a check.
        let mut checked = expanded_call.is_none()
            && signature.generic.is_empty()
            && signature.overloads.is_empty()
            && solution.types.is_empty()
            && solution.unresolved.is_empty()
            && solution.failed.is_empty()
            && solution.invalid.is_empty()
            && !invalid
            && !extra_positionals
            && variadic.is_none()
            && kwargs.is_none()
            && parameters.iter().enumerate().all(|(index, parameter)| {
                parameter.ty.is_some() && (!parameter.required || provided[index])
            });
        for (parameter, argument, value) in arguments {
            let expected = parameters[parameter].ty.map(|ty| {
                if solution.types.is_empty() {
                    ty
                } else {
                    self.context.substitute_inferred_alias(ty, &solution.types)
                }
            });
            if let (Some(expected), Some(actual)) = (
                expected,
                expected.and_then(|expected| {
                    if self
                        .context
                        .unresolved_dictionary_pairs(signature, expected)
                        || (signature.is_unbound_builtin_member()
                            && parameters[parameter].ty.is_some_and(|ty| {
                                self.context
                                    .argument_has_unresolved_variable(ty, &solution.unresolved)
                            }))
                        || self
                            .context
                            .recursive_alias_without_material_parameter(expected)
                    {
                        self.infer(value)
                    } else {
                        self.infer_expected(value, expected)
                    }
                }),
            ) {
                if self.check_deleted_argument(actual, value) {
                    checked = false;
                    continue;
                }
                if !self.context.accepts(expected, actual) {
                    checked = false;
                    let actual = if matches!(value, Expr::List(list) if list.elts.is_empty())
                        && matches!(
                            self.context.node(actual).as_deref(),
                            Some(types::Node::Container(types::Container::List, items))
                                if items.len() == 1 && items[0].is_any()
                        ) {
                        self.context.intern(types::Node::Container(
                            types::Container::List,
                            vec![types::Type::Never],
                        ))
                    } else {
                        actual
                    };
                    if let Some(symbol) =
                        signature.name.as_deref().and_then(types::operators::symbol)
                    {
                        if let Expr::Attribute(attribute) = call.func.as_ref() {
                            if let Some(receiver) = self.infer(&attribute.value) {
                                self.emit(
                                    "operator",
                                    format!(
                                    "Unsupported operand types for {symbol} (\"{}\" and \"{}\")",
                                    self.context.name(receiver), self.context.name(actual)
                                ),
                                    value,
                                );
                                continue;
                            }
                        }
                    }
                    if let Some(symbol) = signature
                        .name
                        .as_deref()
                        .and_then(super::super::super::classes::dataclass::order_symbol)
                    {
                        self.emit(
                            "operator",
                            format!(
                                "Unsupported operand types for {symbol} (\"{}\" and \"{}\")",
                                self.context.name(expected),
                                self.context.name(actual)
                            ),
                            value,
                        );
                        continue;
                    }
                    let target = signature
                        .display_target()
                        .map(|name| format!(" to {name}"))
                        .unwrap_or_default();
                    let hint = self
                        .concatenate_hint(
                            template
                                .parameters
                                .get(parameter)
                                .and_then(|parameter| parameter.ty),
                            expected,
                            actual,
                        )
                        .or_else(|| self.context.variance_hint(expected, actual));
                    let unpacked = self.unpacked_length_argument(original_call, signature, value);
                    let actual = unpacked
                        .as_ref()
                        .map(|(_, ty)| format!("*{}", self.context.name(*ty)))
                        .unwrap_or_else(|| self.context.name(actual));
                    let location = unpacked.map(|(value, _)| value).unwrap_or(value);
                    self.emit_with_hint("arg-type", format!("Argument {argument}{target} has incompatible type \"{actual}\"; expected \"{}\"", self.context.assignment_type_name(expected)), location, hint);
                }
            } else {
                checked = false;
            }
        }
        checked
    }

    fn emit_call_type_variable_errors(
        &mut self,
        call: &ExprCall,
        signature: &Signature,
        solution: &types::constraints::Solution,
    ) {
        for &variable in &solution.failed {
            let message = if let Some(name) = &signature.name {
                let target = if signature.is_unbound_builtin_member() {
                    signature.display_target().expect("named unbound member")
                } else {
                    format!("\"{name}\"")
                };
                format!(
                    "Cannot infer value of type parameter \"{}\" of {target}",
                    self.context.name(variable)
                )
            } else {
                "Cannot infer function type argument".into()
            };
            self.emit("misc", message, call);
        }
        for &(variable, actual) in &solution.invalid {
            let name = signature
                .name
                .as_ref()
                .map(|name| format!("\"{name}\""))
                .unwrap_or_else(|| "function".into());
            self.emit(
                "type-var",
                format!(
                    "Value of type variable \"{}\" of {name} cannot be \"{}\"",
                    self.context.name(variable),
                    self.context.name(actual)
                ),
                call,
            );
        }
    }
}
