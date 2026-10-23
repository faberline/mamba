//! Candidate checks use real child producers and keep errors and types local.
use super::super::generator_inputs::{
    unknown_expression, CandidateFailure, CheckedExpression, CheckedValue, RecoveredCandidate,
    RecoveryCandidateLease, RecoveryOrigin,
};
use super::*;
use std::sync::{Arc, Mutex};
use types::{Node, Type};

pub(in super::super) enum ContextualDispatch {
    Old,
    Handled,
    RecoveredPrepared {
        values: Vec<Type>,
        candidates: Vec<RecoveredCandidate>,
    },
    Prepared {
        values: Vec<Type>,
        placeholders: BTreeSet<usize>,
        all_failed: bool,
    },
}

impl Checker<'_> {
    pub(in super::super) fn check_contextual_generator_overload(
        &mut self,
        call: &ExprCall,
        signature: &Signature,
        expected: Option<Type>,
        binding: Option<&super::super::super::modules::BoundCallable>,
    ) -> ContextualDispatch {
        // Only the real visitor wrapper can publish a source expression lease.
        // With protocol and other synthetic wrappers never establish this key.
        if self.checked_source_call != Some(call as *const ExprCall as usize) {
            return ContextualDispatch::Old;
        }
        if !call
            .arguments
            .args
            .iter()
            .chain(call.arguments.keywords.iter().map(|keyword| &keyword.value))
            .any(|value| matches!(value, Expr::Generator(_) | Expr::DictComp(_)))
        {
            return ContextualDispatch::Old;
        }
        if self.context.has_observed_expression_types() && self.contextual_candidate_depth == 0 {
            self.contextual_unknown(call.range(), "active authoritative expression map");
            return ContextualDispatch::Old;
        }
        let environment = self.environment();
        let bound = binding.is_some_and(|binding| {
            self.scopes
                .candidate_bound_method_role(&call.func, signature, binding, &environment)
                && matches!(call.func.as_ref(), Expr::Attribute(attribute)
                    if matches!(attribute.value.as_ref(), Expr::Name(receiver)
                        if !self.flow.deleted.contains(receiver.id.as_str())
                            && self.candidate_partial_read_is_stable(receiver)))
        });
        if call.arguments.args.iter().any(Expr::is_starred_expr)
            || call
                .arguments
                .keywords
                .iter()
                .any(|keyword| keyword.arg.is_none())
            || !(self
                .scopes
                .candidate_function_role(&call.func, signature, &environment)
                || bound)
        {
            self.contextual_unknown(
                call.range(),
                "unproved ordinary function or argument expansion",
            );
            return ContextualDispatch::Old;
        }
        // Prepare actuals exactly once. Unknown retains these actuals for the
        // old resolver; it must not re-infer them or pretend a trial matched.
        let (values, placeholders, known) =
            self.scopes
                .prepare_overload_values(call, signature, &environment, expected);
        let outcome = if known && placeholders.is_empty() {
            self.run_generator_candidates(call, signature, expected, &values)
        } else {
            unknown_expression(call, "missing prepared actual or contextual empty literal")
        };
        match outcome {
            Ok(CheckedValue::Clean(actual)) => {
                // The ordinary wrapper owns the real Expr allocation and
                // publishes its key only after verifying this call and owner.
                self.checked_call_result = Some(CheckedCallResult {
                    call: call as *const ExprCall as usize,
                    owner: self.scopes.try_input_scope(),
                    actual,
                    expected,
                });
                ContextualDispatch::Handled
            }
            Ok(CheckedValue::Recovered { .. }) => {
                self.contextual_unknown(call.range(), "unbound recovery result");
                ContextualDispatch::Prepared {
                    values,
                    placeholders,
                    all_failed: false,
                }
            }
            Err(CandidateFailure::RecoveryCandidates(candidates)) => {
                ContextualDispatch::RecoveredPrepared { values, candidates }
            }
            Err(CandidateFailure::Rejected) => ContextualDispatch::Prepared {
                values,
                placeholders,
                all_failed: true,
            },
            Err(CandidateFailure::Unknown(range, reason)) => {
                self.contextual_unknown(range, reason);
                ContextualDispatch::Prepared {
                    values,
                    placeholders,
                    all_failed: false,
                }
            }
        }
    }

    pub(in super::super) fn check_candidate_call(
        &mut self,
        call: &ExprCall,
        expected: Option<Type>,
    ) -> CheckedExpression {
        if call.arguments.args.iter().any(Expr::is_starred_expr)
            || call
                .arguments
                .keywords
                .iter()
                .any(|keyword| keyword.arg.is_none())
        {
            return unknown_expression(call, "unpacked candidate call");
        }
        let Some(signature) = self
            .scopes
            .candidate_function(&call.func, &self.environment())
        else {
            return unknown_expression(call, "unproved regular callable producer");
        };
        let Expr::Name(name) = call.func.as_ref() else {
            return unknown_expression(call, "non-Name regular callee");
        };
        self.check_candidate_name_read(name)?;
        let previous = self.expected_value;
        self.expected_value = expected.map(|ty| (call.range(), ty));
        let result = if signature.overloads.is_empty() {
            let (values, placeholders, known) = self.scopes.prepare_overload_values(
                call,
                &signature,
                &self.environment(),
                expected,
            );
            if known && placeholders.is_empty() {
                self.check_regular_candidate_call(call, &signature, &values)
            } else {
                unknown_expression(call, "missing regular prepared argument")
            }
        } else {
            let environment = self.environment();
            let (values, placeholders, known) =
                self.scopes
                    .prepare_overload_values(call, &signature, &environment, expected);
            if !known || !placeholders.is_empty() {
                unknown_expression(call, "unknown nested prepared argument")
            } else {
                match self.run_generator_candidates(call, &signature, expected, &values) {
                    Err(CandidateFailure::RecoveryCandidates(candidates)) => {
                        let fallback = self
                            .scopes
                            .contextual_erased_target(call, &signature, &values);
                        match fallback.and_then(|fallback| {
                            self.take_recovered_fallback(call, &fallback, candidates)
                        }) {
                            Some(value) => Ok(value),
                            None => unknown_expression(call, "unproved nested recovery fallback"),
                        }
                    }
                    Err(CandidateFailure::Rejected) => {
                        let fallback = self
                            .scopes
                            .contextual_erased_target(call, &signature, &values);
                        if let Some(fallback) = fallback {
                            self.check_regular_candidate_call(call, &fallback, &values)
                        } else {
                            // The ordinary checker owns the full all-failed
                            // diagnostic when no erased candidate applies.
                            unknown_expression(call, "no checked nested fallback")
                        }
                    }
                    result => result,
                }
            }
        };
        self.expected_value = previous;
        result
    }

    fn run_generator_candidates(
        &mut self,
        call: &ExprCall,
        signature: &Signature,
        expected: Option<Type>,
        values: &[Type],
    ) -> CheckedExpression {
        // Frozen union/type-overrides retain the old resolver. A represented
        // real Any still permits real rejection, or one uniquely checked match.
        if values
            .iter()
            .any(|&ty| !material_actual(&self.context, ty, 0))
        {
            return unknown_expression(call, "union or opaque prepared actual");
        }
        let has_any = self.scopes.contextual_overload_has_any(values);
        let mut selected = None;
        let mut recovered = Vec::new();
        let mut plausible = false;
        for variant in signature.overloads.iter() {
            if variant
                .parameters
                .iter()
                .any(|parameter| parameter.ty.is_none())
                || !variant.overloads.is_empty()
                || variant.ignored
                || variant.ellipsis
                || variant.constructor && !signature.constructor
                || variant.narrowing.is_some()
            {
                return unknown_expression(call, "unproved candidate shape");
            }
            if self
                .scopes
                .contextual_overload_arguments(call, variant, values)
                .is_none()
            {
                continue;
            }
            if !Self::candidate_variadics_are_unused(call, variant) {
                return unknown_expression(call, "consumed variadic candidate argument");
            }
            plausible = true;
            let previous_types = self.checked_expression_types.take();
            let previous_owner = self.checked_expression_owner;
            let previous_expected = self.expected_value;
            let previous_calls = std::mem::take(&mut self.expected_calls);
            let previous_failed = self.typed_dict_trial_failed.replace(false);
            let previous_depth = self.contextual_candidate_depth;
            let types = Arc::new(Mutex::new(Default::default()));
            self.checked_expression_types = Some(types.clone());
            self.checked_expression_owner = Some(self.scopes.try_input_scope());
            self.contextual_candidate_depth += 1;
            self.expected_value = expected.map(|ty| (call.range(), ty));
            let context = self.context.clone();
            let result = context.with_checked_expression_types(&types, || {
                self.check_regular_candidate_call(call, variant, values)
            });
            // These are all the mutable checker fields used by this finite
            // producer. A nested trial restores the prior failure flag exactly.
            self.checked_expression_types = previous_types;
            self.checked_expression_owner = previous_owner;
            self.expected_value = previous_expected;
            self.expected_calls = previous_calls;
            self.typed_dict_trial_failed = previous_failed;
            self.contextual_candidate_depth = previous_depth;
            match result {
                Ok(value @ CheckedValue::Recovered { .. }) => {
                    // Error-bearing checked data is not an overload match.
                    let lease = if value.has_untyped_origin() {
                        let Some(root_call) = self.checked_source_call else {
                            return unknown_expression(call, "missing recovery source lease");
                        };
                        Some(RecoveryCandidateLease {
                            project: self.source.project,
                            path: self.source.path.clone(),
                            sha256: self.source.sha256.clone(),
                            root_call,
                            call: call as *const ExprCall as usize,
                            owner: self.scopes.try_input_scope(),
                        })
                    } else {
                        None
                    };
                    recovered.push(RecoveredCandidate {
                        signature: variant.clone(),
                        value,
                        types,
                        lease,
                    });
                }
                Ok(CheckedValue::Clean(actual)) => {
                    if has_any {
                        if selected.is_some() {
                            return unknown_expression(call, "multiple checked Any candidates");
                        }
                        selected = Some((actual, types));
                    } else {
                        self.publish_candidate_types(types);
                        return Ok(CheckedValue::Clean(actual));
                    }
                }
                Err(CandidateFailure::RecoveryCandidates(_)) => {
                    return unknown_expression(call, "unresolved nested recovery candidates");
                }
                Err(CandidateFailure::Rejected) => {}
                Err(CandidateFailure::Unknown(range, reason)) => {
                    #[cfg(test)]
                    if reason == "changed checked argument context" {
                        tests::observe_changed_context(
                            self,
                            call,
                            signature,
                            values,
                            selected.is_none(),
                            &mut recovered,
                        );
                    }
                    // A later generic candidate can change its checked item
                    // context after an earlier error-bearing trial. Keep only
                    // the live trial chosen by the ordinary erased fallback.
                    if reason == "changed checked argument context" && selected.is_none() {
                        if let Some(fallback) = self
                            .scopes
                            .contextual_erased_target(call, signature, values)
                        {
                            if let Some(index) = recovered.iter().position(|candidate| {
                                Arc::ptr_eq(&candidate.signature, &fallback)
                                    && self
                                        .recovered_candidate_has_live_untyped_lease(call, candidate)
                            }) {
                                #[cfg(test)]
                                tests::record_changed_context_return(
                                    call,
                                    tests::ReturnKind::Recovered,
                                );
                                return Err(CandidateFailure::RecoveryCandidates(vec![
                                    recovered.remove(index)
                                ]));
                            }
                        }
                    }
                    #[cfg(test)]
                    tests::record_changed_context_return(call, tests::ReturnKind::Unknown);
                    return Err(CandidateFailure::Unknown(range, reason));
                }
            }
        }
        if let Some((actual, types)) = selected {
            self.publish_candidate_types(types);
            return Ok(CheckedValue::Clean(actual));
        }
        if !recovered.is_empty() {
            return Err(CandidateFailure::RecoveryCandidates(recovered));
        }
        if plausible {
            Err(CandidateFailure::Rejected)
        } else {
            unknown_expression(call, "no plausible contextual candidate")
        }
    }

    fn recovered_candidate_has_live_untyped_lease(
        &self,
        call: &ExprCall,
        candidate: &RecoveredCandidate,
    ) -> bool {
        let CheckedValue::Recovered { origins, .. } = &candidate.value else {
            return false;
        };
        if !candidate.value.has_untyped_origin() {
            return false;
        }
        let Some(lease) = candidate.lease.as_ref() else {
            return false;
        };
        lease.project == self.source.project
            && lease.path == self.source.path
            && lease.sha256 == self.source.sha256
            && self.checked_source_call == Some(lease.root_call)
            && lease.call == call as *const ExprCall as usize
            && lease.owner == self.scopes.try_input_scope()
            && self.recovery_origins_are_current(origins)
    }

    pub(super) fn take_recovered_fallback(
        &mut self,
        call: &ExprCall,
        fallback: &Arc<Signature>,
        candidates: Vec<RecoveredCandidate>,
    ) -> Option<CheckedValue> {
        let candidate = candidates
            .into_iter()
            .find(|candidate| Arc::ptr_eq(&candidate.signature, fallback))?;
        let CheckedValue::Recovered { origins, .. } = &candidate.value else {
            return None;
        };
        if candidate.value.has_untyped_origin() {
            let lease = candidate.lease.as_ref()?;
            if lease.project != self.source.project
                || lease.path != self.source.path
                || lease.sha256 != self.source.sha256
                || self.checked_source_call != Some(lease.root_call)
                || lease.call != call as *const ExprCall as usize
                || lease.owner != self.scopes.try_input_scope()
            {
                return None;
            }
        }
        if !self.recovery_origins_are_current(origins) {
            return None;
        }
        for origin in origins {
            match origin {
                RecoveryOrigin::PartialRead(receipt) => self.contextual_unknown(
                    receipt.read,
                    "committed partial dict error recovery remains incomplete",
                ),
                RecoveryOrigin::UntypedCall(receipt) => self.contextual_unknown(
                    receipt.range,
                    "filtered untyped-call error recovery remains incomplete",
                ),
            }
        }
        // This is the actually selected normal erased fallback's checked data.
        // It is never a clean whole-call result or a winning overload trial.
        self.publish_candidate_types(candidate.types);
        Some(candidate.value)
    }

    fn publish_candidate_types(&mut self, types: types::ObservedExpressionTypes) {
        if let Some(previous) = &self.checked_expression_types {
            let checked = types.lock().expect("owned checked expressions");
            previous
                .lock()
                .expect("owned checked expressions")
                .extend(checked.iter().map(|(&pointer, &ty)| (pointer, ty)));
        } else {
            self.checked_expression_types = Some(types);
        }
        self.checked_expression_owner = Some(self.scopes.try_input_scope());
    }

    pub(super) fn contextual_recovery_unknown(&mut self, call: &ExprCall) {
        self.contextual_unknown(call.range(), "selected recovery fallback is unproved");
    }

    fn contextual_unknown(&mut self, range: ruff_text_size::TextRange, reason: &'static str) {
        self.contextual_overload_gaps.insert((
            u32::from(range.start()),
            u32::from(range.end()),
            reason,
        ));
    }
}

fn material_actual(context: &types::Context, ty: Type, depth: usize) -> bool {
    if depth >= 12 {
        return false;
    }
    match ty {
        Type::Bool
        | Type::Int
        | Type::Float
        | Type::Complex
        | Type::Str
        | Type::Bytes
        | Type::None
        | Type::Object
        | Type::Any => true,
        Type::Node(_) => match context.node(ty).as_deref() {
            Some(Node::Container(_, items) | Node::Tuple(items)) => {
                !items.is_empty()
                    && items
                        .iter()
                        .all(|&item| material_actual(context, item, depth + 1))
            }
            Some(Node::Literal(_) | Node::LiteralValue(_)) => true,
            _ => false,
        },
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ports::ProjectStore;
    use crate::domain::source::PythonVersion;
    use crate::infrastructure::{
        filesystem::Filesystem,
        mypy::{blockers, dependencies, modules, roots},
    };
    use mamba_language::application::quality_syntax::python_ast::{Expr, Stmt};
    use serde_json::{json, Value};
    use std::{cell::RefCell, collections::BTreeSet, fs, rc::Rc};

    const NESTED_SOURCE: &str = r#"import json
from pathlib import Path

def read(path):
    return json.loads(path.read_text())

def checked(receipts: list[Path]) -> None:
    assert sum(sum(r["count"] for r in read(path)["results"]) for path in receipts[:2]) == 214
"#;

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum ProbeMode {
        Observe,
        RemoveUntypedOrigin,
        ReplaceCandidateArc,
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub(super) enum ReturnKind {
        Recovered,
        Unknown,
    }

    #[derive(Clone, Debug, Default, PartialEq, Eq)]
    struct CandidateProof {
        present: bool,
        same_arc: bool,
        equal_signature: bool,
        untyped_origins: usize,
        live_untyped_lease: bool,
    }

    #[derive(Clone, Debug)]
    struct Observation {
        selected_none: bool,
        recovered_count: usize,
        fallback_present: bool,
        before: CandidateProof,
        after: CandidateProof,
        mutation_applied: bool,
    }

    #[derive(Clone, Debug)]
    struct ProbeState {
        target_call: usize,
        mode: ProbeMode,
        hits: usize,
        observations: Vec<Observation>,
        returns: Vec<ReturnKind>,
    }

    std::thread_local! {
        static PROBE: RefCell<Option<ProbeState>> = const { RefCell::new(None) };
    }

    struct ProbeGuard {
        previous: Option<ProbeState>,
    }

    impl ProbeGuard {
        fn install(target_call: usize, mode: ProbeMode) -> Self {
            let state = ProbeState {
                target_call,
                mode,
                hits: 0,
                observations: Vec::new(),
                returns: Vec::new(),
            };
            let previous = PROBE.with(|probe| probe.replace(Some(state)));
            Self { previous }
        }

        fn snapshot(&self) -> ProbeState {
            PROBE.with(|probe| probe.borrow().as_ref().expect("installed probe").clone())
        }
    }

    impl Drop for ProbeGuard {
        fn drop(&mut self) {
            let previous = self.previous.take();
            PROBE.with(|probe| {
                probe.replace(previous);
            });
        }
    }

    fn candidate_proof(
        checker: &Checker<'_>,
        call: &ExprCall,
        fallback: Option<&Arc<Signature>>,
        candidate: Option<&RecoveredCandidate>,
    ) -> CandidateProof {
        let Some(candidate) = candidate else {
            return CandidateProof::default();
        };
        let untyped_origins = match &candidate.value {
            CheckedValue::Recovered { origins, .. } => origins
                .iter()
                .filter(|origin| matches!(origin, RecoveryOrigin::UntypedCall(_)))
                .count(),
            CheckedValue::Clean(_) => 0,
        };
        CandidateProof {
            present: true,
            same_arc: fallback.is_some_and(|fallback| Arc::ptr_eq(fallback, &candidate.signature)),
            equal_signature: fallback
                .is_some_and(|fallback| fallback.as_ref() == candidate.signature.as_ref()),
            untyped_origins,
            live_untyped_lease: checker.recovered_candidate_has_live_untyped_lease(call, candidate),
        }
    }

    // This hook sees a real candidate from the normal source visitor. It only
    // removes one proof, then lets the production guard make the decision.
    pub(super) fn observe_changed_context(
        checker: &Checker<'_>,
        call: &ExprCall,
        signature: &Signature,
        values: &[Type],
        selected_none: bool,
        recovered: &mut [RecoveredCandidate],
    ) {
        let call_id = call as *const ExprCall as usize;
        let mode = PROBE.with(|probe| {
            let mut probe = probe.borrow_mut();
            let state = probe.as_mut()?;
            if state.target_call != call_id {
                return None;
            }
            state.hits += 1;
            (state.hits == 1).then_some(state.mode)
        });
        let Some(mode) = mode else {
            return;
        };
        let fallback = checker
            .scopes
            .contextual_erased_target(call, signature, values);
        let index = fallback.as_ref().and_then(|fallback| {
            recovered
                .iter()
                .position(|candidate| Arc::ptr_eq(&candidate.signature, fallback))
        });
        let before = candidate_proof(
            checker,
            call,
            fallback.as_ref(),
            index.and_then(|index| recovered.get(index)),
        );
        let mut mutation_applied = false;
        if let Some(index) = index {
            match mode {
                ProbeMode::Observe => {}
                ProbeMode::RemoveUntypedOrigin => {
                    if let CheckedValue::Recovered { origins, .. } = &mut recovered[index].value {
                        if let Some(origin) = origins
                            .iter()
                            .position(|origin| matches!(origin, RecoveryOrigin::UntypedCall(_)))
                        {
                            origins.remove(origin);
                            mutation_applied = true;
                        }
                    }
                }
                ProbeMode::ReplaceCandidateArc => {
                    let fresh = Arc::new((*recovered[index].signature).clone());
                    recovered[index].signature = fresh;
                    mutation_applied = true;
                }
            }
        }
        let after = candidate_proof(
            checker,
            call,
            fallback.as_ref(),
            index.and_then(|index| recovered.get(index)),
        );
        PROBE.with(|probe| {
            let mut probe = probe.borrow_mut();
            let state = probe.as_mut().expect("installed probe");
            state.observations.push(Observation {
                selected_none,
                recovered_count: recovered.len(),
                fallback_present: fallback.is_some(),
                before,
                after,
                mutation_applied,
            });
        });
    }

    pub(super) fn record_changed_context_return(call: &ExprCall, result: ReturnKind) {
        let call_id = call as *const ExprCall as usize;
        PROBE.with(|probe| {
            let mut probe = probe.borrow_mut();
            if let Some(state) = probe.as_mut().filter(|state| state.target_call == call_id) {
                state.returns.push(result);
            }
        });
    }

    fn run_nested(mode: ProbeMode) -> (Vec<Value>, ProbeState, bool) {
        let root = tempfile::tempdir().unwrap();
        let root_path = fs::canonicalize(root.path()).unwrap();
        fs::write(
            root_path.join("pyproject.toml"),
            "[project]\nname = \"fixture\"\nversion = \"0.0.0\"\nrequires-python = \">=3.12\"\n",
        )
        .unwrap();
        let path = root_path.join("main.py");
        fs::write(&path, NESTED_SOURCE).unwrap();
        let mut workspace = Filesystem.load(std::slice::from_ref(&path), true);
        workspace.projects[0].python_version = PythonVersion::Py312;
        let roots::Prepared { workspace, names } = roots::prepare(&workspace).unwrap();
        let checked_source_count = workspace.sources.len();
        let loaded = dependencies::load(workspace, names);
        assert!(blockers::syntax(&loaded).is_empty());
        assert!(loaded.blocking.is_none());
        let context = Rc::new(types::Context::default());
        let index = loaded.index();
        let installed_sources = (checked_source_count..loaded.workspace.sources.len())
            .filter(|&id| {
                !loaded.local.contains(&id)
                    && !loaded.workspace.sources[id].is_stub
                    && !dependencies::embedded::is_source(&loaded.workspace.sources[id])
            })
            .collect::<BTreeSet<_>>();
        let modules = modules::Modules::build(
            &index,
            context.clone(),
            &loaded.workspace,
            loaded.untyped.clone(),
            loaded.imported.clone(),
            installed_sources,
        );
        let source = &loaded.workspace.sources[0];
        let document = index.modules[0].document.as_ref().expect("parsed source");
        let body = &document.parsed.syntax().body;
        let function = body
            .iter()
            .find_map(|statement| match statement {
                Stmt::FunctionDef(function) if function.name.as_str() == "checked" => {
                    Some(function)
                }
                _ => None,
            })
            .expect("checked function in parsed source");
        let Stmt::Assert(assertion) = &function.body[0] else {
            panic!("nested expression is an assert");
        };
        let Expr::Compare(compare) = assertion.test.as_ref() else {
            panic!("assert compares the nested sum");
        };
        let Expr::Call(outer_call) = &compare.operands[0] else {
            panic!("left operand is the outer sum call");
        };
        let target_call = outer_call as *const ExprCall as usize;
        let probe = ProbeGuard::install(target_call, mode);
        let mut import_notes = BTreeSet::new();
        let mut checker = Checker::new(
            source,
            document,
            context,
            modules::Modules::lookup(&modules, 0),
            &mut import_notes,
        );
        checker.visit_block(body);
        let unknown_gap = checker
            .contextual_overload_gaps
            .iter()
            .any(|(_, _, reason)| *reason == "changed checked argument context");
        let observation = probe.snapshot();
        let diagnostics = checker
            .finish()
            .iter()
            .map(|diagnostic| {
                json!({
                    "rule": diagnostic.rule,
                    "message": diagnostic.message,
                    "hint": diagnostic.hint,
                    "path": diagnostic.path.strip_prefix(&root_path).unwrap(),
                    "severity": diagnostic.severity,
                    "location": {
                        "line": diagnostic.location.line,
                        "column": diagnostic.location.column,
                        "end_line": diagnostic.location.end_line,
                        "end_column": diagnostic.location.end_column,
                    },
                })
            })
            .collect();
        assert_eq!(fs::read(&path).unwrap(), NESTED_SOURCE.as_bytes());
        (diagnostics, observation, unknown_gap)
    }

    fn expected_positive_packet() -> Vec<Value> {
        vec![
            json!({
                "rule": "no-untyped-def",
                "message": "Function is missing a type annotation",
                "hint": null,
                "path": "main.py",
                "severity": "error",
                "location": {"line": 4, "column": 1, "end_line": 5, "end_column": 39},
            }),
            json!({
                "rule": "misc",
                "message": "Generator has incompatible item type \"int\"; expected \"bool\"",
                "hint": null,
                "path": "main.py",
                "severity": "error",
                "location": {"line": 8, "column": 16, "end_line": 8, "end_column": 61},
            }),
            json!({
                "rule": "no-untyped-call",
                "message": "Call to untyped function \"read\" in typed context",
                "hint": null,
                "path": "main.py",
                "severity": "error",
                "location": {"line": 8, "column": 40, "end_line": 8, "end_column": 49},
            }),
        ]
    }

    fn expected_baseline_packet() -> Vec<Value> {
        let mut packet = expected_positive_packet();
        packet.remove(1);
        packet
    }

    fn assert_live_positive(state: &ProbeState) {
        assert_eq!(
            state.hits, 1,
            "the parsed outer sum must reach this arm once"
        );
        assert_eq!(state.returns, [ReturnKind::Recovered]);
        let [observation] = state.observations.as_slice() else {
            panic!("one real candidate observation is required");
        };
        assert!(observation.selected_none);
        assert_eq!(observation.recovered_count, 1);
        assert!(observation.fallback_present);
        assert_eq!(observation.before.untyped_origins, 1);
        assert!(observation.before.same_arc);
        assert!(observation.before.equal_signature);
        assert!(observation.before.live_untyped_lease);
        assert_eq!(observation.before, observation.after);
        assert!(!observation.mutation_applied);
    }

    fn assert_live_before_refusal(state: &ProbeState) -> &Observation {
        assert_eq!(
            state.hits, 1,
            "the parsed outer sum must reach this arm once"
        );
        assert_eq!(state.returns, [ReturnKind::Unknown]);
        let [observation] = state.observations.as_slice() else {
            panic!("one real candidate observation is required");
        };
        assert!(observation.selected_none);
        assert_eq!(observation.recovered_count, 1);
        assert!(observation.fallback_present);
        assert_eq!(observation.before.untyped_origins, 1);
        assert!(observation.before.same_arc);
        assert!(observation.before.equal_signature);
        assert!(observation.before.live_untyped_lease);
        assert!(observation.mutation_applied);
        observation
    }

    #[test]
    fn nested_changed_context_refuses_removed_real_untyped_origin() {
        let (positive, observed, _) = run_nested(ProbeMode::Observe);
        assert_live_positive(&observed);
        assert_eq!(positive, expected_positive_packet());

        let (negative, observed, unknown_gap) = run_nested(ProbeMode::RemoveUntypedOrigin);
        let observation = assert_live_before_refusal(&observed);
        assert_eq!(observation.after.untyped_origins, 0);
        assert!(observation.after.same_arc);
        assert!(!observation.after.live_untyped_lease);
        assert!(unknown_gap);
        assert_eq!(negative, expected_baseline_packet());
    }

    #[test]
    fn nested_changed_context_refuses_equal_but_wrong_candidate_arc() {
        let (positive, observed, _) = run_nested(ProbeMode::Observe);
        assert_live_positive(&observed);
        assert_eq!(positive, expected_positive_packet());

        let (negative, observed, unknown_gap) = run_nested(ProbeMode::ReplaceCandidateArc);
        let observation = assert_live_before_refusal(&observed);
        assert_eq!(observation.after.untyped_origins, 1);
        assert!(!observation.after.same_arc);
        assert!(observation.after.equal_signature);
        assert!(observation.after.live_untyped_lease);
        assert!(unknown_gap);
        assert_eq!(negative, expected_baseline_packet());
    }
}
