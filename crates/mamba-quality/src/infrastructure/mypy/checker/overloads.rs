//! Overload declaration errors retain the original group and implementation spans.
use super::*;
use quality_syntax::python_ast::{ExprCall, StmtFunctionDef};
use signatures::{compatibility, Kind, Signature};
mod generator_candidates;
mod keywords;

impl Checker<'_> {
    pub(super) fn check_overload_call(&mut self, call: &ExprCall, signature: &Signature) {
        self.check_overload_call_with_binding(call, signature, None);
    }

    pub(super) fn check_overload_call_with_binding(
        &mut self,
        call: &ExprCall,
        signature: &Signature,
        binding: Option<&super::super::modules::BoundCallable>,
    ) {
        let expected_return = self
            .expected_value
            .filter(|(range, _)| *range == call.range())
            .map(|(_, ty)| ty)
            .or_else(|| {
                self.expected_calls
                    .get(&(call.start(), call.end()))
                    .copied()
            });
        let mut recovered_candidates = None;
        let resolution = match self.check_contextual_generator_overload(
            call,
            signature,
            expected_return,
            binding,
        ) {
            generator_candidates::ContextualDispatch::Handled => return,
            generator_candidates::ContextualDispatch::Old => {
                self.scopes
                    .resolve_overload(call, signature, &self.environment(), expected_return)
            }
            generator_candidates::ContextualDispatch::RecoveredPrepared { values, candidates } => {
                recovered_candidates = Some(candidates);
                self.scopes.resolve_failed_contextual_overload(
                    call,
                    signature,
                    &self.environment(),
                    expected_return,
                    &values,
                )
            }
            generator_candidates::ContextualDispatch::Prepared {
                values,
                placeholders,
                all_failed,
            } => {
                if all_failed {
                    self.scopes.resolve_failed_contextual_overload(
                        call,
                        signature,
                        &self.environment(),
                        expected_return,
                        &values,
                    )
                } else {
                    self.scopes.resolve_prepared_overload(
                        call,
                        signature,
                        &self.environment(),
                        expected_return,
                        &values,
                        &placeholders,
                    )
                }
            }
        };
        if resolution.union_limit {
            self.emit(
                "misc",
                "Not all union combinations were tried because there are too many unions".into(),
                call,
            );
        }
        if let Some(fallback) = resolution.fallback {
            let recovery = recovered_candidates.take().and_then(|candidates| {
                let value = self.take_recovered_fallback(call, &fallback, candidates);
                if value.is_none() {
                    self.contextual_recovery_unknown(call);
                }
                value
            });
            let recovery_owner = self.scopes.try_input_scope();
            let fallback = Signature {
                name: signature.name.clone(),
                owner: signature.owner.clone(),
                ..(*fallback).clone()
            };
            self.bind_call(call, &fallback);
            if let Some(super::generator_inputs::CheckedValue::Recovered { actual, origins }) =
                recovery
            {
                if self.checked_source_call == Some(call as *const ExprCall as usize)
                    && self.scopes.try_input_scope() == recovery_owner
                    && self.recovery_origins_are_current(&origins)
                {
                    // The normal fallback has now emitted its own diagnostics.
                    // Keep its actual result without claiming a clean match.
                    self.checked_call_result = Some(CheckedCallResult {
                        call: call as *const ExprCall as usize,
                        owner: recovery_owner,
                        actual,
                        expected: expected_return,
                    });
                } else {
                    self.contextual_recovery_unknown(call);
                }
            }
            return;
        }
        if recovered_candidates.is_some() {
            self.contextual_recovery_unknown(call);
        }
        if resolution.matched {
            if signature.definition_module.as_deref() == Some("builtins")
                && matches!(signature.name.as_deref(), Some("dict" | "reversed"))
            {
                if let Some(candidate) = resolution.candidate {
                    self.bind_call(call, &candidate);
                }
            }
            return;
        }
        let target = signature
            .display_target()
            .map(|target| format!(" of {target}"))
            .unwrap_or_default();
        if self.check_overload_keywords(call, signature) {
            return;
        }
        let arguments: Vec<_> = call
            .arguments
            .args
            .iter()
            .chain(call.arguments.keywords.iter().map(|keyword| &keyword.value))
            .map(|value| {
                let value = if let Expr::Starred(starred) = value {
                    starred.value.as_ref()
                } else {
                    value
                };
                let ty = self.infer(value).unwrap_or(types::Type::Any);
                let ty =
                    self.scopes
                        .constructor_argument(signature, value, ty, &self.environment());
                let ty = if matches!(value, Expr::List(list) if list.elts.is_empty()) {
                    self.context.intern(types::Node::Container(
                        types::Container::List,
                        vec![types::Type::Never],
                    ))
                } else {
                    ty
                };
                let alias = self
                    .context
                    .annotation_binding(value, &self.scopes.variables())
                    .and_then(|ty| self.context.alias_id(ty))
                    .and_then(|id| self.context.alias(id).bare_class);
                let bare = alias.or_else(|| {
                    self.context.node(ty).and_then(|node| {
                        if let types::Node::Class(id) = node.as_ref() {
                            Some(*id)
                        } else {
                            None
                        }
                    })
                });
                let display = bare.and_then(|id| {
                    let class = self.context.class(id);
                    (!class.generic.is_empty())
                        .then(|| self.context.applied_class(id, class.generic.clone(), true))
                });
                self.context.name(display.unwrap_or(ty))
            })
            .collect();
        let message = if arguments.is_empty() {
            format!("All overload variants{target} require at least one argument")
        } else {
            let plural = if arguments.len() == 1 {
                "type"
            } else {
                "types"
            };
            format!(
                "No overload variant{target} matches argument {plural} {}",
                arguments
                    .iter()
                    .map(|ty| format!("\"{ty}\""))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        };
        let variants = signature
            .overloads
            .iter()
            .map(|variant| format!("    {}", self.context.pretty_callable(variant)))
            .collect::<Vec<_>>()
            .join("\n");
        let hint = pydantic::field_types::overload_hint(signature, self.python_target)
            .unwrap_or_else(|| format!("Possible overload variants:\n{variants}"));
        self.emit_with_hint("call-overload", message, call, Some(hint));
    }

    pub(super) fn check_overload_groups(&mut self, body: &[Stmt]) {
        let mut groups: BTreeMap<&str, (Vec<&StmtFunctionDef>, bool)> = BTreeMap::new();
        for statement in body {
            let Stmt::FunctionDef(function) = statement else {
                continue;
            };
            if self.scopes.is_overload(function) {
                groups
                    .entry(function.name.as_str())
                    .or_default()
                    .0
                    .push(function);
            } else if let Some(group) = groups.get_mut(function.name.as_str()) {
                group.1 = true;
            }
        }
        for (name, (functions, implementation)) in groups {
            for (index, function) in functions.iter().enumerate() {
                let variables = self
                    .scopes
                    .callable(name)
                    .and_then(|signature| signature.overloads.get(index))
                    .map(|variant| variant.variables.clone())
                    .unwrap_or_else(|| self.scopes.variables());
                self.check_function_variance(
                    function,
                    self.in_class
                        && self.current_method_kind(function)
                            != super::super::classes::MethodKind::Static,
                    &variables,
                );
            }
            let first = functions[0];
            let start = first.decorator_list.first().unwrap().expression.start();
            for (decorator, kind) in [
                ("staticmethod", super::super::classes::MethodKind::Static),
                ("classmethod", super::super::classes::MethodKind::Class),
            ] {
                let count = functions
                    .iter()
                    .filter(|function| super::super::classes::method_kind(function) == kind)
                    .count();
                if count != 0 && count != functions.len() {
                    self.emit("misc", format!("Overload does not consistently use the \"@{decorator}\" decorator on all function signatures."), &ruff_text_size::TextRange::at(start, ruff_text_size::TextSize::new(1)));
                }
            }
            if functions.len() == 1 {
                let end = if self.source.is_stub {
                    first.end()
                } else {
                    start + ruff_text_size::TextSize::new(1)
                };
                self.emit(
                    "misc",
                    "Single overload definition, multiple required".into(),
                    &ruff_text_size::TextRange::new(start, end),
                );
            } else if !implementation && !self.source.is_stub && !self.protocol_stub(first) {
                self.emit(
                    "no-overload-impl",
                    "An overloaded function outside a stub file must have an implementation".into(),
                    &ruff_text_size::TextRange::at(start, ruff_text_size::TextSize::new(1)),
                );
            }
            if self.in_class {
                self.check_method_override(first);
            }
            let Some(signature) = self.scopes.callable(name).cloned() else {
                continue;
            };
            for (later, second) in signature.overloads.iter().enumerate() {
                for (earlier, first) in signature.overloads.iter().take(later).enumerate() {
                    let receiver = |function: &StmtFunctionDef, variant: &Signature| {
                        usize::from(
                            self.in_class
                                && variant.bound_receiver.is_none()
                                && (self.current_method_kind(function)
                                    != super::super::classes::MethodKind::Static
                                    || function.name.as_str() == "__new__"),
                        )
                    };
                    let any_inputs = first
                        .parameters
                        .iter()
                        .skip(receiver(functions[earlier], first))
                        .chain(
                            second
                                .parameters
                                .iter()
                                .skip(receiver(functions[later], second)),
                        )
                        .any(|parameter| parameter.ty.unwrap_or(types::Type::Any).is_any());
                    // Any is compatible with a concrete type during assignment,
                    // but is not more precise than that type for reachability.
                    if self.context.parameters_cover(second, first, false)
                        && (!any_inputs || Self::overload_any_inputs_more_precise(second, first))
                    {
                        self.emit("overload-cannot-match", format!("Overloaded function signature {} will never be matched: signature {}'s parameter type(s) are the same or broader", later + 1, earlier + 1), &self.function_range(functions[later]));
                        break;
                    }
                    // Coverage inference cannot replace the partial-overlap predicate.
                    if !(self.in_class && name == "__get__")
                        && compatibility::parameters_overlap(&self.context, first, second, false)
                        // A pure Any fallback does not prove partial widening.
                        && (!any_inputs || !self.context.parameters_cover(first, second, false))
                        && !self.context.accepts(
                            second.returns.unwrap_or(types::Type::Any),
                            first.returns.unwrap_or(types::Type::Any),
                        )
                    {
                        self.emit("overload-overlap", format!("Overloaded function signatures {} and {} overlap with incompatible return types", earlier + 1, later + 1), &self.function_range(functions[earlier]));
                    }
                }
            }
        }
    }

    fn overload_any_inputs_more_precise(parent: &Signature, child: &Signature) -> bool {
        if parent.ellipsis || child.ellipsis {
            return true;
        }
        // Use the same parameter correspondence as assignment compatibility.
        // The caller already checked ordinary types and the complete call domain.
        let positional: Vec<_> = child
            .parameters
            .iter()
            .filter(|parameter| {
                matches!(
                    parameter.kind,
                    Kind::PositionalOnly | Kind::PositionalOrKeyword
                )
            })
            .collect();
        let varargs = child
            .parameters
            .iter()
            .find(|parameter| parameter.kind == Kind::VariadicPositional);
        let kwargs = child
            .parameters
            .iter()
            .find(|parameter| parameter.kind == Kind::VariadicKeyword);
        let mut position = 0;
        for parameter in parent.parameters.iter() {
            let target = match parameter.kind {
                Kind::PositionalOnly | Kind::PositionalOrKeyword => {
                    let target = positional.get(position).copied().or(varargs);
                    position += 1;
                    target
                }
                Kind::KeywordOnly => child
                    .parameters
                    .iter()
                    .find(|target| {
                        matches!(target.kind, Kind::KeywordOnly | Kind::PositionalOrKeyword)
                            && target.name == parameter.name
                    })
                    .or(kwargs),
                Kind::VariadicPositional => varargs,
                Kind::VariadicKeyword => kwargs,
            };
            let Some(target) = target else {
                return false;
            };
            if parameter.ty.unwrap_or(types::Type::Any).is_any()
                && !target.ty.unwrap_or(types::Type::Any).is_any()
            {
                return false;
            }
        }
        true
    }

    pub(super) fn check_overload_implementation(
        &mut self,
        function: &StmtFunctionDef,
        signature: &Signature,
    ) {
        let range = function.decorator_list.first().map_or_else(
            || self.function_range(function),
            |decorator| {
                ruff_text_size::TextRange::new(decorator.expression.start(), function.end())
            },
        );
        for (index, variant) in signature.overloads.iter().enumerate() {
            if !compatibility::parameters_accept(&self.context, variant, signature, false) {
                self.emit("misc", format!("Overloaded function implementation does not accept all possible parameters of signature {}", index + 1), &range);
            } else if !self.context.accepts(
                signature.returns.unwrap_or(types::Type::Any),
                variant.returns.unwrap_or(types::Type::Any),
            ) {
                self.emit("misc", format!("Overloaded function implementation cannot produce return type of signature {}", index + 1), &range);
            }
        }
    }
}
