//! Ordered overload inference follows the pinned union-search depth limit.
use super::*;
use crate::infrastructure::mypy::types::constraints::FactoryAmbiguity;
use mamba_language::application::quality_syntax::python_ast::ExprCall;
use std::collections::BTreeSet;
mod binding;
mod unions;

pub(crate) struct OverloadResult {
    pub matched: bool,
    pub returns: Type,
    pub fallback: Option<Arc<Signature>>,
    pub candidate: Option<Arc<Signature>>,
    pub union_limit: bool,
    pub factory_poly: Option<FactoryAmbiguity>,
}

impl OverloadResult {
    fn matched(returns: Type) -> Self {
        Self {
            matched: true,
            returns,
            fallback: None,
            candidate: None,
            union_limit: false,
            factory_poly: None,
        }
    }
}

impl Scopes {
    pub(crate) fn resolve_overload(
        &self,
        call: &ExprCall,
        signature: &Signature,
        environment: &BTreeMap<String, Type>,
        expected_return: Option<Type>,
    ) -> OverloadResult {
        self.resolve_overload_with_lambda_shape(call, signature, environment, expected_return, true)
    }

    pub(in crate::infrastructure::mypy) fn resolve_overload_with_lambda_shape(
        &self,
        call: &ExprCall,
        signature: &Signature,
        environment: &BTreeMap<String, Type>,
        expected_return: Option<Type>,
        lambda_shape_eligible: bool,
    ) -> OverloadResult {
        let lambda_shape_eligible = lambda_shape_eligible
            && !call.arguments.args.iter().any(Expr::is_starred_expr)
            && call
                .arguments
                .keywords
                .iter()
                .all(|keyword| keyword.arg.is_some());
        let expanded = self.collection_expanded_call(call, signature);
        let call = expanded.as_deref().unwrap_or(call);
        if call.arguments.args.iter().any(Expr::is_starred_expr)
            || (!matches!(signature.name.as_deref(), Some("sorted" | "sum" | "zip"))
                && call
                    .arguments
                    .keywords
                    .iter()
                    .any(|keyword| keyword.arg.is_none()))
        {
            return OverloadResult::matched(Type::Any);
        }
        let (values, placeholders, _) =
            self.prepare_overload_values(call, signature, environment, expected_return);
        self.resolve_prepared_overload_inner(
            call,
            signature,
            environment,
            expected_return,
            (&values, &placeholders),
            lambda_shape_eligible,
        )
    }

    pub(in crate::infrastructure::mypy) fn prepare_overload_values(
        &self,
        call: &ExprCall,
        signature: &Signature,
        environment: &BTreeMap<String, Type>,
        expected_return: Option<Type>,
    ) -> (Vec<Type>, BTreeSet<usize>, bool) {
        let mut known = true;
        let inferred_values: Vec<_> = call
            .arguments
            .args
            .iter()
            .chain(call.arguments.keywords.iter().map(|keyword| &keyword.value))
            .enumerate()
            .map(|(index, value)| {
                if crate::infrastructure::mypy::pydantic::field_types::is_field(signature)
                    && index >= call.arguments.args.len()
                    && call.arguments.keywords[index - call.arguments.args.len()]
                        .arg
                        .as_ref()
                        .is_some_and(|name| name.as_str() == "default_factory")
                {
                    if let Some((_, ty)) = self.field_factory_type(
                        value,
                        environment,
                        expected_return.filter(|&ty| self.field_context(ty)),
                    ) {
                        return (ty, false);
                    }
                }
                let actual = self
                    .context
                    .infer_with_calls(value, environment, &|expression| {
                        self.expression_type(expression, environment)
                    });
                let empty_list_any_placeholder = actual.is_some_and(|ty| {
                    matches!(value, Expr::List(list) if list.elts.is_empty())
                        && self.context.observed_expression_type(value).is_none()
                        && matches!(
                            self.context.node(ty).as_deref(),
                            Some(types::Node::Container(types::Container::List, items))
                                if items == &[Type::Any]
                        )
                });
                known &= actual.is_some();
                let actual = actual.unwrap_or(Type::Any);
                let constructor_actual =
                    self.constructor_argument(signature, value, actual, environment);
                (
                    constructor_actual,
                    empty_list_any_placeholder && constructor_actual == actual,
                )
            })
            .collect();
        let mut empty_list_any_placeholders = BTreeSet::new();
        let values: Vec<_> = inferred_values
            .into_iter()
            .enumerate()
            .map(|(index, (ty, placeholder))| {
                if placeholder {
                    empty_list_any_placeholders.insert(index);
                }
                ty
            })
            .collect();
        (values, empty_list_any_placeholders, known)
    }

    pub(in crate::infrastructure::mypy) fn resolve_prepared_overload(
        &self,
        call: &ExprCall,
        signature: &Signature,
        environment: &BTreeMap<String, Type>,
        expected_return: Option<Type>,
        values: &[Type],
        placeholders: &BTreeSet<usize>,
    ) -> OverloadResult {
        self.resolve_prepared_overload_inner(
            call,
            signature,
            environment,
            expected_return,
            (values, placeholders),
            true,
        )
    }

    fn resolve_prepared_overload_inner(
        &self,
        call: &ExprCall,
        signature: &Signature,
        environment: &BTreeMap<String, Type>,
        expected_return: Option<Type>,
        inputs: (&[Type], &BTreeSet<usize>),
        lambda_shape_eligible: bool,
    ) -> OverloadResult {
        let (values, empty_list_any_placeholders) = inputs;
        let forced = unions::force_optional_split(self, call, signature, values);
        let unioned = if values
            .iter()
            .any(|&ty| unions::items(&self.context, ty).is_some())
        {
            unions::Search {
                scopes: self,
                call,
                signature,
                empty_list_any_placeholders,
                expected_return,
                forced,
            }
            .resolve(values, 0)
        } else {
            Ok(None)
        };
        let direct = self.overload_direct(
            call,
            signature,
            values,
            empty_list_any_placeholders,
            expected_return,
            lambda_shape_eligible,
        );
        let interrupted = unioned.is_err();
        let unioned = unioned.ok().flatten();
        match (direct, unioned) {
            (Some((direct, candidate, _)), Some(unioned)) => {
                let result = if !direct.is_any() && !forced && self.context.accepts(unioned, direct)
                {
                    direct
                } else {
                    unioned
                };
                return OverloadResult {
                    candidate: Some(candidate),
                    ..OverloadResult::matched(result)
                };
            }
            (Some((result, candidate, factory_poly)), None) => {
                return OverloadResult {
                    candidate: Some(candidate),
                    factory_poly: if !interrupted && !forced {
                        factory_poly
                    } else {
                        None
                    },
                    ..OverloadResult::matched(result)
                };
            }
            (None, Some(result)) => return OverloadResult::matched(result),
            _ => {}
        }
        self.overload_failure(
            call,
            signature,
            environment,
            expected_return,
            values,
            interrupted && !forced,
        )
    }

    pub(in crate::infrastructure::mypy) fn resolve_failed_contextual_overload(
        &self,
        call: &ExprCall,
        signature: &Signature,
        environment: &BTreeMap<String, Type>,
        expected_return: Option<Type>,
        values: &[Type],
    ) -> OverloadResult {
        self.overload_failure(call, signature, environment, expected_return, values, false)
    }

    fn overload_failure(
        &self,
        call: &ExprCall,
        signature: &Signature,
        environment: &BTreeMap<String, Type>,
        expected_return: Option<Type>,
        values: &[Type],
        union_limit: bool,
    ) -> OverloadResult {
        let fallback = signature
            .overloads
            .iter()
            .find(|variant| {
                binding::pairs(&self.context, call, variant, values, false).is_some_and(|pairs| {
                    pairs.iter().all(|&(expected, actual)| {
                        self.context
                            .reversed_erased_argument(signature, expected, actual)
                            .unwrap_or_else(|| {
                                binding::erased_accepts(&self.context, expected, actual)
                            })
                    })
                })
            })
            .cloned();
        let returns = fallback
            .as_ref()
            .map(|variant| {
                let solution = self.call_solution(call, variant, environment, expected_return);
                self.context
                    .substitute(variant.returns.unwrap_or(Type::Any), &solution.types)
            })
            .unwrap_or(Type::Any);
        OverloadResult {
            matched: false,
            returns,
            fallback,
            candidate: None,
            union_limit,
            factory_poly: None,
        }
    }

    pub(in crate::infrastructure::mypy) fn contextual_overload_arguments(
        &self,
        call: &ExprCall,
        signature: &Signature,
        values: &[Type],
    ) -> Option<Vec<(Type, Type)>> {
        if signature
            .parameters
            .iter()
            .any(|parameter| parameter.ty.is_none())
        {
            return None;
        }
        binding::pairs(&self.context, call, signature, values, true)
    }

    pub(in crate::infrastructure::mypy) fn contextual_overload_has_any(
        &self,
        values: &[Type],
    ) -> bool {
        values
            .iter()
            .any(|&ty| self.overload_has_real_any(ty, false))
    }

    pub(in crate::infrastructure::mypy) fn contextual_erased_target(
        &self,
        call: &ExprCall,
        signature: &Signature,
        values: &[Type],
    ) -> Option<Arc<Signature>> {
        signature
            .overloads
            .iter()
            .find(|variant| {
                binding::pairs(&self.context, call, variant, values, false).is_some_and(|pairs| {
                    pairs.iter().all(|&(expected, actual)| {
                        self.context
                            .reversed_erased_argument(signature, expected, actual)
                            .unwrap_or_else(|| {
                                binding::erased_accepts(&self.context, expected, actual)
                            })
                    })
                })
            })
            .cloned()
    }

    fn overload_direct(
        &self,
        call: &ExprCall,
        signature: &Signature,
        values: &[Type],
        empty_list_any_placeholders: &BTreeSet<usize>,
        expected_return: Option<Type>,
        lambda_shape_eligible: bool,
    ) -> Option<(Type, Arc<Signature>, Option<FactoryAmbiguity>)> {
        let track_any_formals = values.iter().enumerate().any(|(index, &ty)| {
            !empty_list_any_placeholders.contains(&index) && self.overload_has_real_any(ty, false)
        }) && !call.arguments.args.iter().any(Expr::is_starred_expr)
            && call
                .arguments
                .keywords
                .iter()
                .all(|keyword| keyword.arg.is_some());
        let returns: Vec<_> = signature
            .overloads
            .iter()
            .filter_map(|variant| {
                let pairs = binding::pairs(&self.context, call, variant, values, true)?;
                let solution =
                    self.context
                        .collection_call_solution(variant, &pairs, expected_return);
                if !solution.failed.is_empty() || !solution.invalid.is_empty() {
                    return None;
                }
                if !pairs
                    .iter()
                    .enumerate()
                    .all(|(index, &(expected, actual))| {
                        let expected = self.context.substitute(expected, &solution.types);
                        let expression = call.arguments.args.get(index).or_else(|| {
                            index
                                .checked_sub(call.arguments.args.len())
                                .and_then(|index| call.arguments.keywords.get(index))
                                .map(|keyword| &keyword.value)
                        });
                        if let Some(Expr::Lambda(lambda)) =
                            expression.filter(|_| lambda_shape_eligible)
                        {
                            let contextual = self.lambda_expected_context(lambda, expected);
                            if matches!(
                                self.context.node(contextual).as_deref(),
                                Some(types::Node::Callable(_))
                            ) && self.context.callable_signature(contextual).is_some_and(
                                |target| self.lambda_context_mismatch(lambda, contextual, &target),
                            ) {
                                return false;
                            }
                        }
                        let actual = expression.map_or(actual, |expression| {
                            if let Expr::Lambda(lambda) = expression {
                                if let Some(actual) =
                                    self.lambda_type_expected(lambda, &self.environment(), expected)
                                {
                                    return actual;
                                }
                            }
                            self.context.constructor_contextual_argument(
                                signature, expected, expression, actual,
                            )
                        });
                        self.context.accepts(expected, actual)
                            || expression.is_some_and(|expression| {
                                self.contextual_literal_collection_matches(
                                    expected, expression, actual,
                                )
                            })
                    })
                {
                    return None;
                }
                let returned = self
                    .context
                    .substitute(variant.returns.unwrap_or(Type::Any), &solution.types);
                if signature.definition_module.as_deref() == Some("builtins")
                    && signature.name.as_deref() == Some("reversed")
                    && expected_return
                        .is_some_and(|expected| !self.context.accepts(expected, returned))
                {
                    return None;
                }
                // pairs emits one entry per direct actual, in values order.
                // Starred/mapping calls retain the conservative ambiguity path.
                let formals = (track_any_formals && pairs.len() == values.len()).then(|| {
                    pairs
                        .iter()
                        .map(|&(expected, _)| self.context.substitute(expected, &solution.types))
                        .collect::<Vec<_>>()
                });
                Some((returned, variant.clone(), formals, solution.factory_poly))
            })
            .collect();
        let (first, candidate, formals, factory_poly) = returns.first()?;
        let ambiguous = values.iter().enumerate().any(|(index, &actual)| {
            if empty_list_any_placeholders.contains(&index) {
                return false;
            }
            if !self.overload_has_real_any(actual, true) {
                return false;
            }
            let Some(expected) = formals.as_ref().and_then(|formals| formals.get(index)) else {
                return true;
            };
            returns.iter().skip(1).any(|(_, _, formals, _)| {
                !formals
                    .as_ref()
                    .and_then(|formals| formals.get(index))
                    .is_some_and(|&formal| self.context.any_origin_equal(formal, *expected))
            })
        });
        Some((
            if ambiguous
                && returns
                    .iter()
                    .any(|(ty, _, _, _)| !self.context.any_origin_equal(*ty, *first))
                && !(signature.definition_module.as_deref() == Some("builtins")
                    && signature.name.as_deref() == Some("dict"))
            {
                Type::Any
            } else {
                *first
            },
            candidate.clone(),
            if !ambiguous
                && matches!(call.arguments.args.as_ref(), [argument] if !argument.is_starred_expr())
                && call.arguments.keywords.is_empty()
            {
                factory_poly.clone()
            } else {
                None
            },
        ))
    }

    // checkexpr.py3063 scans type-object callables; 6913 excludes them.
    // True is positive evidence from stored types only. Unrepresented alias
    // expansion, constructor signatures and other metadata retain the old gate.
    fn overload_has_real_any(&self, ty: Type, ignore_in_type_obj: bool) -> bool {
        fn query_signature(
            context: &types::Context,
            signature: &Signature,
            ignore: bool,
            seen: &mut BTreeSet<Type>,
        ) -> bool {
            if !signature.overloads.is_empty() {
                return signature
                    .overloads
                    .iter()
                    .any(|variant| query_signature(context, variant, ignore, seen));
            }
            // Native constructor metadata alone cannot prove a type-object
            // Callable. Preserve the old gate when that distinction is needed.
            if ignore && signature.constructor {
                return false;
            }
            signature
                .parameters
                .iter()
                .filter_map(|parameter| parameter.ty)
                .chain(signature.returns)
                .any(|ty| query(context, ty, ignore, seen))
        }
        fn query(
            context: &types::Context,
            ty: Type,
            ignore: bool,
            seen: &mut BTreeSet<Type>,
        ) -> bool {
            if ty.is_real_any() {
                return true;
            }
            if !seen.insert(ty) {
                return false;
            }
            match context.node(ty).as_deref() {
                Some(
                    types::Node::Container(_, items)
                    | types::Node::Tuple(items)
                    | types::Node::Union(items),
                ) => items.iter().any(|&ty| query(context, ty, ignore, seen)),
                Some(
                    types::Node::VariadicTuple(ty)
                    | types::Node::Unpack(ty)
                    | types::Node::ParamSpecArgs(ty)
                    | types::Node::ParamSpecKwargs(ty),
                ) => query(context, *ty, ignore, seen),
                Some(types::Node::Parameters(parameters, _)) => parameters
                    .iter()
                    .filter_map(|parameter| parameter.ty)
                    .any(|ty| query(context, ty, ignore, seen)),
                Some(types::Node::Variable(variable)) => std::iter::once(variable.bound)
                    .chain(variable.default)
                    .chain(
                        variable
                            .values
                            .iter()
                            .copied()
                            .filter(|_| !variable.parameter_spec && !variable.type_var_tuple),
                    )
                    .any(|ty| query(context, ty, ignore, seen)),
                Some(types::Node::Callable(callable)) => {
                    query_signature(context, callable, ignore, seen)
                }
                Some(types::Node::AppliedClass {
                    id,
                    arguments,
                    value: false,
                }) => {
                    let class = context.class(*id);
                    if let Some(dictionary) = &class.typed_dict {
                        class.generic.is_empty()
                            && dictionary
                                .items
                                .iter()
                                .any(|item| query(context, item.ty, ignore, seen))
                    } else {
                        arguments.iter().any(|&ty| query(context, ty, ignore, seen))
                    }
                }
                Some(types::Node::Instance(id)) => {
                    let class = context.class(*id);
                    class.generic.is_empty()
                        && class.typed_dict.as_ref().is_some_and(|dictionary| {
                            dictionary
                                .items
                                .iter()
                                .any(|item| query(context, item.ty, ignore, seen))
                        })
                }
                // A class value is excluded at the ambiguity gate. At the
                // tracking gate its full constructor Callable remains unproved.
                Some(types::Node::Class(_) | types::Node::AppliedClass { value: true, .. }) => {
                    false
                }
                _ => false,
            }
        }
        query(&self.context, ty, ignore_in_type_obj, &mut BTreeSet::new())
    }

    fn contextual_literal_collection_matches(
        &self,
        expected: Type,
        expression: &Expr,
        actual: Type,
    ) -> bool {
        let items = match expression {
            Expr::List(list) => &list.elts,
            Expr::Tuple(tuple) => &tuple.elts,
            _ => return false,
        };
        let Some(expected_item) = self.context.collection_literal_item(expected) else {
            return false;
        };
        !items.is_empty()
            && self.context.iterable_item(actual).is_some()
            && items.iter().all(|item| {
                self.context
                    .literal_expression(item)
                    .is_some_and(|literal| self.context.accepts(expected_item, literal))
            })
    }
}
