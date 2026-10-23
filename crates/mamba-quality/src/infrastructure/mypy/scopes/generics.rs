//! Resolve generic returns from the caller's lexical value types.
use super::super::signatures::Kind;
use super::*;
mod calls;
use mamba_language::application::quality_syntax::python_ast::ExprCall;

impl Scopes {
    // Use the same lexical producer for raw and contextual dict results.
    pub(in crate::infrastructure::mypy) fn dictionary_comprehension_scope(
        &self,
        dictionary: &mamba_language::application::quality_syntax::python_ast::ExprDictComp,
        environment: &BTreeMap<String, Type>,
    ) -> Option<(Self, BTreeMap<String, Type>)> {
        self.comprehension_scope(&dictionary.generators, environment)
    }

    // A flat candidate consumes actual checked values, never re-infers them.
    pub(in crate::infrastructure::mypy) fn checked_call_solution(
        &self,
        call: &ExprCall,
        signature: &Signature,
        actuals: &[Type],
        expected_return: Option<Type>,
    ) -> Option<super::super::types::constraints::Solution> {
        if call.arguments.args.iter().any(Expr::is_starred_expr)
            || call
                .arguments
                .keywords
                .iter()
                .any(|keyword| keyword.arg.is_none())
            || signature.parameters.iter().any(|parameter| {
                parameter.ty.is_none()
                    || matches!(
                        parameter.kind,
                        Kind::VariadicPositional | Kind::VariadicKeyword
                    )
            })
            || actuals.len() != call.arguments.args.len() + call.arguments.keywords.len()
        {
            return None;
        }
        let mut pairs = self.contextual_overload_arguments(call, signature, actuals)?;
        for ((expected, actual), value) in pairs.iter_mut().zip(call.arguments.args.iter()) {
            *actual = self
                .context
                .recursive_input_literal(value, *actual, *expected);
        }
        if signature.generic.is_empty() {
            return Some(self.context.solve(&[], &[]));
        }
        let expected_return = expected_return
            .and_then(|ty| self.context.collection_constructor_context(signature, ty));
        Some(
            self.context
                .collection_call_solution(signature, &pairs, expected_return),
        )
    }

    pub(crate) fn call_solution(
        &self,
        call: &ExprCall,
        signature: &Signature,
        environment: &BTreeMap<String, Type>,
        expected_return: Option<Type>,
    ) -> super::super::types::constraints::Solution {
        if signature.generic.is_empty() {
            return self.context.solve(&[], &[]);
        }
        let expected_return = expected_return
            .and_then(|ty| self.context.collection_constructor_context(signature, ty));
        let positional: Vec<_> = signature
            .parameters
            .iter()
            .filter(|parameter| {
                matches!(
                    parameter.kind,
                    Kind::PositionalOnly | Kind::PositionalOrKeyword
                )
            })
            .collect();
        let variadic = signature
            .parameters
            .iter()
            .find(|parameter| parameter.kind == Kind::VariadicPositional);
        let kwargs = signature
            .parameters
            .iter()
            .find(|parameter| parameter.kind == Kind::VariadicKeyword);
        let mut pairs = Vec::new();
        let mut deferred_lambdas = Vec::new();
        let contextual_lambda = |expression: &Expr, expected: Type, actual: Type| {
            if !matches!(expression, Expr::Lambda(_)) {
                return false;
            }
            let (Some(expected), Some(actual)) =
                (self.context.node(expected), self.context.node(actual))
            else {
                return false;
            };
            let (types::Node::Callable(expected), types::Node::Callable(actual)) =
                (expected.as_ref(), actual.as_ref())
            else {
                return false;
            };
            actual.implicit
                && actual.return_explicit
                && !expected.ellipsis
                && !actual.ellipsis
                && expected.narrowing.is_none()
                && expected.generic.is_empty()
                && actual.generic.is_empty()
                && expected.overloads.is_empty()
                && actual.overloads.is_empty()
                && expected.parameters.len() == actual.parameters.len()
                && expected
                    .parameters
                    .iter()
                    .zip(actual.parameters.iter())
                    .all(|(expected, actual)| {
                        matches!(
                            expected.kind,
                            Kind::PositionalOnly | Kind::PositionalOrKeyword
                        ) && matches!(
                            actual.kind,
                            Kind::PositionalOnly | Kind::PositionalOrKeyword
                        )
                    })
        };
        if let Some(pair) = variadic.and_then(|parameter| {
            self.tuple_call_pair(call, parameter, positional.len(), environment)
        }) {
            pairs.push(pair);
        }
        for (index, value) in call.arguments.args.iter().enumerate() {
            if let Some(expected) = positional
                .get(index)
                .copied()
                .or(variadic)
                .and_then(|parameter| parameter.ty)
            {
                if index >= positional.len() && self.context.unpack_target(expected).is_some() {
                    continue;
                }
                let source = if let Expr::Starred(starred) = value {
                    starred.value.as_ref()
                } else {
                    value
                };
                if let Some(actual) = self.context.infer_with_calls(source, environment, &|call| {
                    self.expression_type(call, environment)
                }) {
                    let actual = self.constructor_argument(signature, source, actual, environment);
                    let actual = if matches!(value, Expr::Starred(_)) {
                        let Some(item) = self.context.iterable_item(actual) else {
                            continue;
                        };
                        item
                    } else {
                        actual
                    };
                    let actual = if self.context.contains_literal(expected) {
                        self.context.literal_expression(source).unwrap_or(actual)
                    } else {
                        actual
                    };
                    let pair = pairs.len();
                    pairs.push((
                        expected,
                        self.context
                            .recursive_input_literal(source, actual, expected),
                    ));
                    if !matches!(value, Expr::Starred(_))
                        && contextual_lambda(source, expected, pairs[pair].1)
                    {
                        deferred_lambdas.push((pair, source, expected, true));
                    }
                }
            }
        }
        for keyword in &call.arguments.keywords {
            let Some(name) = &keyword.arg else {
                if signature.definition_module.as_deref() == Some("builtins")
                    && signature.name.as_deref() == Some("enumerate")
                {
                    if let Some((_, value)) = self
                        .context
                        .infer_with_calls(&keyword.value, environment, &|value| {
                            self.expression_type(value, environment)
                        })
                        .and_then(|ty| self.context.mapping_types(ty))
                    {
                        pairs.extend(
                            signature
                                .parameters
                                .iter()
                                .filter(|parameter| {
                                    matches!(
                                        parameter.kind,
                                        Kind::PositionalOrKeyword | Kind::KeywordOnly
                                    )
                                })
                                .filter_map(|parameter| {
                                    parameter.ty.map(|expected| (expected, value))
                                }),
                        );
                    }
                }
                continue;
            };
            let parameter = signature
                .parameters
                .iter()
                .find(|parameter| {
                    parameter.name == name.as_str()
                        && matches!(
                            parameter.kind,
                            Kind::PositionalOrKeyword | Kind::KeywordOnly
                        )
                })
                .or(kwargs);
            if let Some(expected) = parameter.and_then(|parameter| parameter.ty) {
                if let Some(actual) =
                    self.context
                        .infer_with_calls(&keyword.value, environment, &|call| {
                            self.expression_type(call, environment)
                        })
                {
                    let actual =
                        self.constructor_argument(signature, &keyword.value, actual, environment);
                    let actual = if self.context.contains_literal(expected) {
                        self.context
                            .literal_expression(&keyword.value)
                            .unwrap_or(actual)
                    } else {
                        actual
                    };
                    let pair = pairs.len();
                    pairs.push((expected, actual));
                    if contextual_lambda(&keyword.value, expected, actual) {
                        deferred_lambdas.push((pair, &keyword.value, expected, false));
                    }
                }
            }
        }
        if !deferred_lambdas.is_empty() {
            // Keep the original pairs as fallback and preserve their final order.
            // A context-free lambda's Any return cannot seed its own output binder.
            let ordinary: Vec<_> = pairs
                .iter()
                .enumerate()
                .filter(|(index, _)| !deferred_lambdas.iter().any(|(pair, _, _, _)| pair == index))
                .map(|(_, &pair)| pair)
                .collect();
            let preliminary =
                self.context
                    .collection_call_solution(signature, &ordinary, expected_return);
            if preliminary.failed.is_empty() && preliminary.invalid.is_empty() {
                for (pair, expression, expected, positional) in deferred_lambdas {
                    let Expr::Lambda(lambda) = expression else {
                        continue;
                    };
                    let target = self.context.callable_signature(expected).unwrap();
                    let inputs_known = target.parameters.iter().all(|parameter| {
                        parameter.ty.is_some_and(|ty| {
                            !self
                                .context
                                .argument_has_unresolved_variable(ty, &preliminary.unresolved)
                                && !matches!(
                                    self.context.substitute(ty, &preliminary.types),
                                    Type::Any | Type::AnySpecial | Type::Never
                                )
                        })
                    });
                    if !inputs_known {
                        continue;
                    }
                    let contextual = self.context.substitute(expected, &preliminary.types);
                    let Some(actual) = self.lambda_type_expected(lambda, environment, contextual)
                    else {
                        continue;
                    };
                    let actual =
                        self.constructor_argument(signature, expression, actual, environment);
                    let actual = if self.context.contains_literal(expected) {
                        self.context
                            .literal_expression(expression)
                            .unwrap_or(actual)
                    } else {
                        actual
                    };
                    let actual = if positional {
                        self.context
                            .recursive_input_literal(expression, actual, expected)
                    } else {
                        actual
                    };
                    pairs[pair] = (expected, actual);
                }
            }
        }
        let mut solution =
            self.context
                .collection_call_solution(signature, &pairs, expected_return);
        if signature.definition_module.as_deref() == Some("builtins")
            && (signature.name.as_deref() == Some("dict") || signature.is_unbound_builtin_member())
            && !solution.failed.is_empty()
        {
            for &variable in signature.generic.iter() {
                solution.types.insert(variable, Type::Any);
            }
        }
        if let Some((prefix, variable)) = self.context.signature_parameter_spec(signature) {
            if solution.unresolved.contains(&variable) {
                let captured = call
                    .arguments
                    .args
                    .iter()
                    .skip(prefix)
                    .map(|value| (String::new(), Kind::PositionalOnly, value))
                    .chain(call.arguments.keywords.iter().filter_map(|keyword| {
                        Some((
                            keyword.arg.as_ref()?.to_string(),
                            Kind::PositionalOrKeyword,
                            &keyword.value,
                        ))
                    }))
                    .map(|(name, kind, value)| super::super::signatures::Parameter {
                        name,
                        kind,
                        required: true,
                        ty: Some(
                            self.context
                                .infer_with_calls(value, environment, &|value| {
                                    self.expression_type(value, environment)
                                })
                                .unwrap_or(Type::Any),
                        ),
                    })
                    .collect();
                solution.types.insert(
                    variable,
                    self.context
                        .intern(types::Node::Parameters(captured, false)),
                );
                solution.unresolved.retain(|&ty| ty != variable);
            }
        }
        solution
    }

    pub(crate) fn expression_type(
        &self,
        expression: &Expr,
        environment: &BTreeMap<String, Type>,
    ) -> Option<Type> {
        match expression {
            Expr::TString(_) => Some(
                self.context
                    .template_instance(self.context.module_target(&self.namespace)),
            ),
            Expr::Name(name) => self.name_expression_type(name),
            Expr::Lambda(lambda) => self.lambda_type(lambda, environment),
            Expr::If(conditional) => {
                let infer = |value: &Expr, branch: &BTreeMap<String, Type>| {
                    self.context.infer_with_calls(value, branch, &|value| {
                        self.expression_type(value, branch)
                    })
                };
                self.context.infer_conditional_arms(
                    conditional,
                    environment,
                    &infer,
                    &|test, positive, branch| {
                        self.context.refine_condition_environment_with(
                            test,
                            positive,
                            branch,
                            &infer,
                            &|call, positive, branch| {
                                self.runtime_predicate_refinement(
                                    call,
                                    positive,
                                    branch,
                                    &|value| infer(value, branch),
                                )
                            },
                        )
                    },
                )
            }
            Expr::Generator(generator) => self.comprehension_generator_type(generator, environment),
            Expr::Dict(dictionary) => self.context.infer_dictionary_collection_join(
                dictionary,
                self.context.module_target(&self.namespace),
                environment,
                &|value| self.expression_type(value, environment),
            ),
            Expr::ListComp(list) => {
                let (scope, environment) =
                    self.comprehension_scope(&list.generators, environment)?;
                let item = self.context.infer_with_scoped_conditional(
                    &list.elt,
                    &environment,
                    &|value| scope.expression_type(value, &environment),
                )?;
                // Generic comprehension inference removes a redundant bool arm
                // when the same inferred union already contains primitive int.
                let item = match self.context.node(item).as_deref() {
                    Some(types::Node::Union(items))
                        if items.contains(&Type::Bool) && items.contains(&Type::Int) =>
                    {
                        self.context.union(
                            items
                                .iter()
                                .copied()
                                .filter(|&ty| ty != Type::Bool)
                                .collect(),
                        )
                    }
                    _ => item,
                };
                Some(
                    self.context
                        .intern(types::Node::Container(types::Container::List, vec![item])),
                )
            }
            Expr::DictComp(dictionary) => {
                let (scope, environment) =
                    self.dictionary_comprehension_scope(dictionary, environment)?;
                let key = self.context.infer_with_calls(
                    dictionary.key.as_deref()?,
                    &environment,
                    &|value| scope.expression_type(value, &environment),
                )?;
                let value =
                    self.context
                        .infer_with_calls(&dictionary.value, &environment, &|value| {
                            scope.expression_type(value, &environment)
                        })?;
                Some(self.context.intern(types::Node::Container(
                    types::Container::Dict,
                    vec![key, value],
                )))
            }
            Expr::SetComp(set)
                if types::supported_set_comprehension_generators(&set.generators) =>
            {
                let (scope, environment) = self.comprehension_scope_with_item(
                    &set.generators,
                    environment,
                    &|ty| match self.context.node(ty).as_deref() {
                        Some(types::Node::Tuple(items)) => Some(self.context.union(items.clone())),
                        _ => self.context.iterable_item(ty),
                    },
                    false,
                )?;
                let item = self
                    .context
                    .infer_with_calls(&set.elt, &environment, &|value| {
                        scope.expression_type(value, &environment)
                    })?;
                Some(
                    self.context
                        .intern(types::Node::Container(types::Container::Set, vec![item])),
                )
            }
            Expr::Compare(compare) => {
                let pairs = self.comparison_details(compare, environment)?;
                pairs
                    .into_iter()
                    .map(|pair| pair.returns)
                    .reduce(|left, right| self.context.join(left, right))
            }
            Expr::Call(call) => self.call_return(call, environment),
            Expr::Slice(slice) => Some(self.slice_expression_type(slice, environment)),
            Expr::Subscript(subscript) => self
                .builtin_constructor_expression(expression)
                .map(|signature| self.context.callable_type(signature))
                .or_else(|| {
                    self.nominal_subscript_call(subscript, environment)
                        .and_then(|call| self.call_return(&call, environment))
                }),
            Expr::Yield(_) | Expr::YieldFrom(_) => {
                self.generator_expression_type(expression, environment)
            }
            Expr::Attribute(attribute) => {
                self.attribute_expression_type(expression, attribute, environment)
            }
            _ => None,
        }
    }
}
