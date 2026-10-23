use super::*;
use quality_syntax::python_ast::{ExprCall, StmtFunctionDef};
use signatures::{Kind, Signature};
mod arguments;
mod awaiting;
mod casts;
mod dictionary;
mod generators;
mod missing;
mod unpacked;

#[derive(Clone, Copy)]
pub(super) enum UntypedCallPolicy {
    WholeCallee,
    AnonymousAlias,
    UnionMember,
    // Implicit with protocol calls do not pass through Mypy visit_call_expr.
    WithProtocol,
}

fn newly_filtered_untyped_call(
    before: Option<bool>,
    after: Option<bool>,
    message: String,
) -> Option<String> {
    (before == Some(false) && after == Some(true)).then_some(message)
}

impl Checker<'_> {
    pub(super) fn check_regular_candidate_call(
        &mut self,
        call: &ExprCall,
        source_signature: &std::sync::Arc<Signature>,
        prepared: &[types::Type],
    ) -> super::generator_inputs::CheckedExpression {
        use super::generator_inputs::{unknown_expression, RecoveryOrigin, UntypedCallRecovery};
        let signature = source_signature.as_ref();
        let owner = self.scopes.try_input_scope();
        let binding = (signature.implicit
            && signature.generic.is_empty()
            && signature.returns == Some(types::Type::Any)
            && signature
                .parameters
                .iter()
                .all(|parameter| parameter.ty.is_none())
            && self.contextual_candidate_depth != 0
            && self.typed_dict_trial_failed == Some(false)
            && self.checked_expression_owner == Some(owner)
            && self.checked_expression_types.is_some()
            && self.checked_source_call.is_some())
        .then(|| {
            self.scopes
                .candidate_source_callable(&call.func, source_signature, &self.environment())
        })
        .flatten();
        let filtered = if signature.implicit {
            self.emit_untyped_call_outcome(call, signature, UntypedCallPolicy::WholeCallee)
        } else {
            None
        };
        let recovery = match (binding, filtered, self.checked_source_call) {
            (Some(binding), Some(message), Some(root_call)) => {
                // Only this original false -> true emit is transferred. Any
                // pre-existing or later failure keeps its ordinary refusal.
                self.typed_dict_trial_failed = Some(false);
                Some(UntypedCallRecovery {
                    project: self.source.project,
                    path: self.source.path.clone(),
                    sha256: self.source.sha256.clone(),
                    root_call,
                    call: call as *const ExprCall as usize,
                    range: call.range(),
                    binding,
                    message,
                })
            }
            _ => None,
        };
        // typeops.py callable_type gives a truly unannotated FuncDef Any
        // parameters. A missing actual is never converted here.
        let dynamic = (signature.implicit
            && signature.generic.is_empty()
            && signature.returns == Some(types::Type::Any)
            && signature
                .parameters
                .iter()
                .all(|parameter| parameter.ty.is_none()))
        .then(|| Signature {
            parameters: signature
                .parameters
                .iter()
                .map(|parameter| signatures::Parameter {
                    ty: Some(types::Type::Any),
                    ..parameter.clone()
                })
                .collect::<Vec<_>>()
                .into(),
            ..signature.clone()
        });
        let signature = dynamic.as_ref().unwrap_or(signature);
        if signature.returns == Some(types::Type::None)
            && signature.return_explicit
            && self.none_allowed != Some(call.range())
        {
            self.emit(
                "func-returns-value",
                format!(
                    "{} does not return a value (it only ever returns None)",
                    signature
                        .display_target()
                        .unwrap_or_else(|| "Function".into())
                ),
                call,
            );
        }
        let result = self.bind_candidate_call(call, signature, prepared);
        match (result, recovery) {
            (Ok(value), Some(receipt)) => {
                if value.actual() != types::Type::Any
                    || self.scopes.try_input_scope() != owner
                    || self.checked_expression_owner != Some(owner)
                    || !self.recovery_origins_are_current(&[RecoveryOrigin::UntypedCall(
                        receipt.clone(),
                    )])
                {
                    return unknown_expression(call, "unproved checked untyped-call recovery");
                }
                Ok(value.with_origin(RecoveryOrigin::UntypedCall(receipt)))
            }
            (result, _) => result,
        }
    }

    pub(super) fn check_defaults(&mut self, function: &StmtFunctionDef) {
        for parameter in function.parameters.iter_non_variadic_params() {
            let Some(value) = parameter.default.as_deref() else {
                continue;
            };
            self.visit_expr(value);
            let Some(expected) = parameter
                .parameter
                .annotation
                .as_deref()
                .and_then(|expression| self.scopes.annotation(expression))
            else {
                continue;
            };
            if let Some(actual) = self.infer_expected(value, expected) {
                if !self.context.accepts(expected, actual) {
                    self.emit("assignment", format!("Incompatible default for parameter \"{}\" (default has type \"{}\", parameter has type \"{}\")", parameter.parameter.name, self.context.name(actual), self.context.name(expected)), value);
                }
            }
        }
    }

    pub(super) fn check_function_call(&mut self, expression: &Expr) {
        let previous = self.checked_call_result.take();
        let previous_source = self.checked_source_call.take();
        self.checked_source_call = match expression {
            Expr::Call(call)
                if self
                    .checked_normal_visits
                    .contains(&(expression as *const Expr as usize)) =>
            {
                Some(call as *const ExprCall as usize)
            }
            _ => None,
        };
        let _ = self.check_function_call_with_policy(expression, UntypedCallPolicy::WholeCallee);
        if let (Expr::Call(call), Some(result)) = (expression, self.checked_call_result.take()) {
            if result.call == call as *const ExprCall as usize
                && result.owner == self.scopes.try_input_scope()
            {
                if let Some(values) = &self.checked_expression_types {
                    values
                        .lock()
                        .expect("owned checked expressions")
                        .insert(expression as *const Expr as usize, Some(result.actual));
                    self.checked_expression_contexts
                        .insert(expression as *const Expr as usize, result.expected);
                }
            }
        }
        self.checked_call_result = previous;
        self.checked_source_call = previous_source;
    }

    pub(super) fn check_with_protocol_call(
        &mut self,
        expression: &Expr,
    ) -> Option<std::sync::Arc<Signature>> {
        self.check_function_call_with_policy(expression, UntypedCallPolicy::WithProtocol)
    }

    fn check_function_call_with_policy(
        &mut self,
        expression: &Expr,
        untyped_call_policy: UntypedCallPolicy,
    ) -> Option<std::sync::Arc<Signature>> {
        if self.check_generator_expression(expression) {
            return None;
        }
        if let Expr::Await(awaited) = expression {
            self.check_await(awaited);
            return None;
        }
        let Expr::Call(call) = expression else {
            return None;
        };
        if self.check_partial_set_add(call) {
            return None;
        }
        if self.check_typing_cast(call) {
            return None;
        }
        if self.check_dataclass_field_call(call) {
            return None;
        }
        if self.check_dataclass_replace_call(call) {
            return None;
        }
        if self.check_alias_reveal(call) {
            return None;
        }
        if self.check_type_alias_call(call) {
            return None;
        }
        if self.check_type_object_call(call, untyped_call_policy) {
            return None;
        }
        if matches!(call.func.as_ref(), Expr::Attribute(attribute) if attribute.attr.as_str() == "__init__" && self.infer(&attribute.value).is_some_and(|ty| self.context.class_id(ty).is_some() && !self.context.is_class_value(ty)))
        {
            return None;
        }
        if self.check_protocol_call(call) {
            return None;
        }
        if self.check_typed_dict_call(call) {
            return None;
        }
        if self.called_model(&call.func).is_some() {
            return None;
        }
        if let Some(node) = self.infer(&call.func).and_then(|ty| self.context.node(ty)) {
            if let types::Node::Union(items) = node.as_ref() {
                let policy = self.union_untyped_call_policy(call, items);
                let policy = if matches!(untyped_call_policy, UntypedCallPolicy::WithProtocol) {
                    untyped_call_policy
                } else {
                    policy
                };
                self.check_union_call(call, items, policy);
                return None;
            }
        }
        // The legacy declaration checker owns this exact outer call only.
        // visit_expression still walks its callee and every nested argument.
        if self.type_parameter_declaration_call == Some(call.range()) {
            return None;
        }
        let Some((signature, binding)) = self
            .scopes
            .callable_expression_in_with_binding(&call.func, &self.environment())
        else {
            if let Some(ty) = self.infer(&call.func) {
                if !matches!(
                    ty,
                    types::Type::Any | types::Type::AnySpecial | types::Type::Never
                ) {
                    self.emit(
                        "operator",
                        format!("\"{}\" not callable", self.context.name(ty)),
                        call,
                    );
                }
            }
            return None;
        };
        self.check_signature_call_checked_with_binding(
            call,
            signature,
            untyped_call_policy,
            binding.as_ref(),
        )
    }

    fn union_untyped_call_policy(
        &self,
        call: &ExprCall,
        items: &[types::Type],
    ) -> UntypedCallPolicy {
        let (member, policy) = match call.func.as_ref() {
            Expr::Attribute(attribute) => (
                Some(attribute.attr.as_str()),
                UntypedCallPolicy::WholeCallee,
            ),
            Expr::Name(name)
                if self
                    .scopes
                    .inferred_binding_identity(name.id.as_str())
                    .is_some_and(|(_, origin)| origin < name.start()) =>
            {
                (None, UntypedCallPolicy::AnonymousAlias)
            }
            _ => return UntypedCallPolicy::UnionMember,
        };
        let mut first = None;
        if self.same_implicit_member_union(items, member, &mut first) {
            policy
        } else {
            UntypedCallPolicy::UnionMember
        }
    }

    fn same_implicit_member_union(
        &self,
        items: &[types::Type],
        member: Option<&str>,
        first: &mut Option<std::sync::Arc<Signature>>,
    ) -> bool {
        if items.is_empty() {
            return false;
        }
        for &ty in items {
            let Some(node) = self.context.node(ty) else {
                return false;
            };
            let signature = match node.as_ref() {
                types::Node::Union(nested) => {
                    if !self.same_implicit_member_union(nested, member, first) {
                        return false;
                    }
                    continue;
                }
                types::Node::Callable(signature) => signature,
                _ => return false,
            };
            // Original member lookup simplifies identical implicit method shapes.
            // Native signatures can still differ in owner and source metadata.
            if !signature.implicit
                || !signature.plain_source_implicit_name
                || signature.name.is_none()
                || member.is_some_and(|member| signature.name.as_deref() != Some(member))
                || signature.owner.is_none()
                || signature.bound_receiver.is_none()
                || signature.constructor
                || signature.ignored
                || signature.ellipsis
                || signature.return_explicit
                || signature.returns.is_none_or(|ty| !ty.is_any())
                || signature.narrowing.is_some()
                || !signature.generic.is_empty()
                || !signature.overloads.is_empty()
                || signature
                    .parameters
                    .iter()
                    .any(|parameter| parameter.ty.is_some_and(|ty| !ty.is_any()))
            {
                return false;
            }
            if let Some(previous) = first {
                if previous.name != signature.name
                    || previous.parameters.len() != signature.parameters.len()
                    || previous
                        .parameters
                        .iter()
                        .zip(signature.parameters.iter())
                        .any(|(left, right)| {
                            left.name != right.name
                                || left.kind != right.kind
                                || left.required != right.required
                        })
                {
                    return false;
                }
            } else {
                *first = Some(signature.clone());
            }
        }
        first.is_some()
    }

    fn check_union_call(
        &mut self,
        call: &ExprCall,
        items: &[types::Type],
        policy: UntypedCallPolicy,
    ) {
        for &ty in items {
            if let Some(node) = self.context.node(ty) {
                if let types::Node::Union(nested) = node.as_ref() {
                    self.check_union_call(call, nested, policy);
                    continue;
                }
            }
            if matches!(
                ty,
                types::Type::Any | types::Type::AnySpecial | types::Type::Never
            ) {
                continue;
            }
            if self.check_class_value_call(ty, call) {
                continue;
            }
            if let Some(signature) = self
                .context
                .union_callable_signature_for(ty, self.python_target)
            {
                self.check_signature_call(call, signature, policy);
            } else if let Some(rule) = self.context.non_callable_rule_for(ty, self.python_target) {
                self.emit(
                    rule,
                    format!("\"{}\" not callable", self.context.name(ty)),
                    call,
                );
            }
        }
    }

    fn check_signature_call(
        &mut self,
        call: &ExprCall,
        signature: std::sync::Arc<Signature>,
        untyped_call_policy: UntypedCallPolicy,
    ) {
        let _ = self.check_signature_call_checked(call, signature, untyped_call_policy);
    }

    fn check_signature_call_checked(
        &mut self,
        call: &ExprCall,
        signature: std::sync::Arc<Signature>,
        untyped_call_policy: UntypedCallPolicy,
    ) -> Option<std::sync::Arc<Signature>> {
        self.check_signature_call_checked_with_binding(call, signature, untyped_call_policy, None)
    }

    fn check_signature_call_checked_with_binding(
        &mut self,
        call: &ExprCall,
        signature: std::sync::Arc<Signature>,
        untyped_call_policy: UntypedCallPolicy,
        binding: Option<&super::super::modules::BoundCallable>,
    ) -> Option<std::sync::Arc<Signature>> {
        if self.check_dictionary_expression_call(call, &signature) {
            return None;
        }
        if signature.ignored {
            return None;
        }
        self.check_any_constructor(&signature, call);
        if !signature.overloads.is_empty() {
            self.check_deleted_overload_arguments(call);
            if !self.check_typed_dict_method(call) {
                self.check_overload_call_with_binding(call, &signature, binding);
            }
            return None;
        }
        if signature.implicit
            && !matches!(
                untyped_call_policy,
                UntypedCallPolicy::UnionMember | UntypedCallPolicy::WithProtocol
            )
        {
            self.emit_untyped_call(call, &signature, untyped_call_policy);
        }
        if signature.returns == Some(types::Type::None)
            && signature.return_explicit
            && !signature.constructor
            && (signature.name.is_some() || self.scopes.declared_callable(&call.func))
            && self.none_allowed != Some(call.range())
            && !matches!(call.func.as_ref(), Expr::Attribute(attribute) if self.infer(&attribute.value).is_some_and(|ty| self.context.typed_dict(ty).is_some()))
        {
            self.emit(
                "func-returns-value",
                format!(
                    "{} does not return a value (it only ever returns None)",
                    signature
                        .display_target()
                        .unwrap_or_else(|| "Function".into())
                ),
                call,
            );
        }
        let checked = if !self.check_typed_dict_method(call) {
            if matches!(untyped_call_policy, UntypedCallPolicy::WithProtocol) {
                self.bind_call_checked(call, &signature)
            } else {
                self.bind_call(call, &signature);
                false
            }
        } else {
            false
        };
        self.check_from_attributes_call(call);
        checked.then_some(signature)
    }

    fn emit_untyped_call(
        &mut self,
        call: &ExprCall,
        signature: &Signature,
        policy: UntypedCallPolicy,
    ) {
        let _ = self.emit_untyped_call_outcome(call, signature, policy);
    }

    fn emit_untyped_call_outcome(
        &mut self,
        call: &ExprCall,
        signature: &Signature,
        policy: UntypedCallPolicy,
    ) -> Option<String> {
        let previous_failed = self.typed_dict_trial_failed;
        let first = self.diagnostics.len();
        let target = if matches!(policy, UntypedCallPolicy::AnonymousAlias) {
            // Inferred callable aliases lose their diagnostic name in the original.
            "(unknown)".into()
        } else {
            signature
                .untyped_call_target()
                .unwrap_or_else(|| "(unknown)".into())
        };
        let message = format!("Call to untyped function {target} in typed context");
        self.emit("no-untyped-call", message.clone(), call);
        let filtered =
            newly_filtered_untyped_call(previous_failed, self.typed_dict_trial_failed, message);
        let Some(emitted) = self.diagnostics.get(first).cloned() else {
            return filtered;
        };
        let Some(end) = emitted.location.end_offset else {
            return filtered;
        };
        // Calls are visited before their callee. Keep the actual inner error's span
        // when the existing report deduplication will retain the outer error.
        for previous in &mut self.diagnostics[..first] {
            if previous.path == emitted.path
                && previous.tool == emitted.tool
                && previous.rule == emitted.rule
                && previous.message == emitted.message
                && previous.severity == emitted.severity
                && emitted.location.offset.is_some()
                && previous.location.offset == emitted.location.offset
                && previous
                    .location
                    .end_offset
                    .is_some_and(|outer| outer > end)
            {
                previous.location = emitted.location.clone();
            }
        }
        filtered
    }
}

#[cfg(test)]
mod tests {
    use super::newly_filtered_untyped_call;

    #[test]
    fn recovery_requires_one_new_filtered_untyped_error() {
        assert_eq!(
            newly_filtered_untyped_call(Some(false), Some(true), "error".into()).as_deref(),
            Some("error")
        );
        for (before, after) in [
            (None, None),
            (None, Some(true)),
            (Some(true), Some(true)),
            (Some(false), Some(false)),
            (Some(false), None),
        ] {
            assert_eq!(
                newly_filtered_untyped_call(before, after, "error".into()),
                None,
                "before={before:?}, after={after:?}"
            );
        }
    }
}
