//! Bind unpacked positional values to every parameter they can reach.
use super::*;

impl Checker<'_> {
    pub(super) fn check_starred_call_arguments(&mut self, call: &ExprCall, signature: &Signature) {
        // A type-variable tuple is bound by the generic call solver. Its unpacked
        // elements have no independent concrete argument type to check here.
        if signature.parameters.iter().any(|parameter| {
            parameter
                .ty
                .is_some_and(|ty| self.contains_type_var_tuple(ty))
        }) {
            return;
        }
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
        let keyword_variadic = parameters
            .iter()
            .position(|parameter| parameter.kind == Kind::VariadicKeyword);
        let mut provided = vec![false; parameters.len()];
        let mut position = 0;
        let mut too_many = false;

        for argument in &call.arguments.args {
            if let Expr::Starred(starred) = argument {
                let actual = self.infer(&starred.value);
                if let Some(Some(types::Node::Tuple(items))) =
                    actual.map(|ty| self.context.node(ty).as_deref().cloned())
                {
                    for _ in items {
                        if let Some(&index) = positional.get(position) {
                            provided[index] = true;
                        } else if let Some(index) = variadic {
                            provided[index] = true;
                        } else {
                            too_many = true;
                        }
                        position += 1;
                    }
                } else {
                    for &index in positional.iter().skip(position) {
                        provided[index] = true;
                    }
                    if let Some(index) = variadic {
                        provided[index] = true;
                    }
                    position = positional.len();
                }
            } else {
                if let Some(&index) = positional.get(position) {
                    provided[index] = true;
                } else if let Some(index) = variadic {
                    provided[index] = true;
                } else {
                    too_many = true;
                }
                position += 1;
            }
        }
        if too_many {
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
        }
        let mut invalid = false;
        for keyword in &call.arguments.keywords {
            let Some(name) = keyword.arg.as_ref().map(|name| name.as_str()) else {
                for (index, parameter) in parameters.iter().enumerate() {
                    if matches!(
                        parameter.kind,
                        Kind::PositionalOrKeyword | Kind::KeywordOnly | Kind::VariadicKeyword
                    ) {
                        provided[index] = true;
                    }
                }
                continue;
            };
            let parameter = parameters
                .iter()
                .position(|parameter| {
                    parameter.name == name
                        && matches!(
                            parameter.kind,
                            Kind::PositionalOrKeyword | Kind::KeywordOnly
                        )
                })
                .or(keyword_variadic);
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
        }
        self.check_missing_call_arguments(call, signature, &provided, invalid, too_many);

        let solution = self
            .scopes
            .call_solution(call, signature, &self.environment(), None);
        self.emit_call_type_variable_errors(call, signature, &solution);
        self.check_starred_positional_types(
            call,
            signature,
            &positional,
            variadic,
            &solution.types,
        );
        for (keyword_index, keyword) in call.arguments.keywords.iter().enumerate() {
            let Some(name) = keyword.arg.as_ref().map(|name| name.as_str()) else {
                self.check_starred_keyword_mapping(
                    call,
                    signature,
                    keyword_index,
                    &keyword.value,
                    &solution.types,
                );
                continue;
            };
            let parameter = parameters
                .iter()
                .find(|parameter| {
                    parameter.name == name
                        && matches!(
                            parameter.kind,
                            Kind::PositionalOrKeyword | Kind::KeywordOnly
                        )
                })
                .or_else(|| keyword_variadic.map(|index| &parameters[index]));
            if let Some(expected) = parameter
                .and_then(|parameter| parameter.ty)
                .map(|ty| self.context.substitute_inferred_alias(ty, &solution.types))
            {
                if let Some(actual) = self.infer_expected(&keyword.value, expected) {
                    self.emit_starred_argument_type(
                        signature,
                        &format!("\"{name}\""),
                        &keyword.value,
                        expected,
                        actual,
                        false,
                    );
                }
            }
        }
    }

    fn check_starred_keyword_mapping(
        &mut self,
        call: &ExprCall,
        signature: &Signature,
        keyword_index: usize,
        value: &Expr,
        solutions: &std::collections::BTreeMap<types::Type, types::Type>,
    ) {
        let Some(actual) = self.infer(value) else {
            return;
        };
        let Some((_, item)) = self.context.mapping_types(actual) else {
            return;
        };
        let target = signature
            .display_target()
            .map(|name| format!(" to {name}"))
            .unwrap_or_default();
        for parameter in signature.parameters.iter().filter(|parameter| {
            matches!(
                parameter.kind,
                Kind::PositionalOrKeyword | Kind::KeywordOnly
            )
        }) {
            let Some(expected) = parameter
                .ty
                .map(|ty| self.context.substitute_inferred_alias(ty, solutions))
            else {
                continue;
            };
            if self.context.accepts(expected, item) {
                continue;
            }
            self.emit(
                "arg-type",
                format!(
                    "Argument {}{target} has incompatible type \"**{}\"; expected \"{}\"",
                    call.arguments.args.len() + keyword_index + 1,
                    self.context.name(actual),
                    self.context.assignment_type_name(expected)
                ),
                value,
            );
        }
    }

    fn check_starred_positional_types(
        &mut self,
        call: &ExprCall,
        signature: &Signature,
        positional: &[usize],
        variadic: Option<usize>,
        solutions: &std::collections::BTreeMap<types::Type, types::Type>,
    ) {
        let mut position = 0;
        let mut unknown_star = false;
        for (argument_index, argument) in call.arguments.args.iter().enumerate() {
            if let Expr::Starred(starred) = argument {
                let Some(actual) = self.infer(&starred.value) else {
                    continue;
                };
                if let Some(types::Node::Tuple(items)) = self.context.node(actual).as_deref() {
                    let items = items.clone();
                    for item in items {
                        let parameter = positional.get(position).copied().or(variadic);
                        if let Some(expected) = parameter
                            .and_then(|index| signature.parameters[index].ty)
                            .map(|ty| self.context.substitute_inferred_alias(ty, solutions))
                        {
                            self.emit_starred_argument_type(
                                signature,
                                &(argument_index + 1).to_string(),
                                &starred.value,
                                expected,
                                item,
                                true,
                            );
                        }
                        position += 1;
                    }
                } else if let Some(item) = self.context.iterable_item(actual) {
                    let mut expected_types = Vec::new();
                    for &index in positional.iter().skip(position).chain(variadic.iter()) {
                        if let Some(expected) = signature.parameters[index]
                            .ty
                            .map(|ty| self.context.substitute_inferred_alias(ty, solutions))
                        {
                            if !expected_types.contains(&expected) {
                                expected_types.push(expected);
                            }
                        }
                    }
                    for expected in expected_types {
                        self.emit_starred_argument_type(
                            signature,
                            &(argument_index + 1).to_string(),
                            &starred.value,
                            expected,
                            item,
                            true,
                        );
                    }
                    unknown_star = true;
                    position = positional.len();
                }
            } else {
                if unknown_star && variadic.is_none() {
                    continue;
                }
                let parameter = positional.get(position).copied().or(variadic);
                if let Some(expected) = parameter
                    .and_then(|index| signature.parameters[index].ty)
                    .map(|ty| self.context.substitute_inferred_alias(ty, solutions))
                {
                    if let Some(actual) = self.infer_expected(argument, expected) {
                        self.emit_starred_argument_type(
                            signature,
                            &(argument_index + 1).to_string(),
                            argument,
                            expected,
                            actual,
                            false,
                        );
                    }
                }
                position += 1;
            }
        }
    }

    fn emit_starred_argument_type(
        &mut self,
        signature: &Signature,
        number: &str,
        value: &Expr,
        expected: types::Type,
        actual: types::Type,
        starred: bool,
    ) {
        if self.context.accepts(expected, actual) {
            return;
        }
        let target = signature
            .display_target()
            .map(|name| format!(" to {name}"))
            .unwrap_or_default();
        let prefix = if starred { "*" } else { "" };
        let actual_name = if starred {
            // A starred argument reports its container, not one of its items.
            self.infer(value)
                .map(|ty| self.context.name(ty))
                .unwrap_or_else(|| self.context.name(actual))
        } else {
            self.context.name(actual)
        };
        self.emit_with_hint(
            "arg-type",
            format!(
                "Argument {number}{target} has incompatible type \"{prefix}{actual_name}\"; expected \"{}\"",
                self.context.assignment_type_name(expected)
            ),
            value,
            (!starred)
                .then(|| self.context.variance_hint(expected, actual))
                .flatten(),
        );
    }

    fn contains_type_var_tuple(&self, ty: types::Type) -> bool {
        match self.context.node(ty).as_deref() {
            Some(types::Node::Variable(variable)) => variable.type_var_tuple,
            Some(types::Node::Unpack(item) | types::Node::VariadicTuple(item)) => {
                self.contains_type_var_tuple(*item)
            }
            Some(types::Node::Tuple(items)) => {
                items.iter().any(|&item| self.contains_type_var_tuple(item))
            }
            _ => false,
        }
    }
}
