//! Loop passes repeat until flow and shared declarations reach a fixed point.
use super::*;
use ruff_text_size::{TextRange, TextSize};
use types::Type;

pub(super) enum Entry<'a> {
    For {
        target: &'a Expr,
        item: Option<Type>,
        invariant_columns: Vec<(&'a Expr, Type)>,
        start: TextSize,
    },
    While(&'a Expr),
}

// Only visit bookkeeping is restored. Lexical types, partial constraints, read
// commitments, model bindings and immutable interned nodes remain live.
#[derive(Clone, PartialEq, Eq)]
pub(in crate::infrastructure::mypy::checker) struct Bookkeeping {
    annotations: Vec<BTreeMap<String, TextSize>>,
    final_initializers: BTreeSet<TextSize>,
    invalid_alias_uses: BTreeSet<(u32, usize, usize)>,
    expected_calls: BTreeMap<(TextSize, TextSize), Type>,
    parameter_spec_errors: BTreeSet<Type>,
    partial_dict_lvalues: BTreeSet<TextSize>,
    partial_list_lvalues: BTreeSet<TextSize>,
}

impl Bookkeeping {
    pub(in crate::infrastructure::mypy::checker) fn save(checker: &Checker<'_>) -> Self {
        Self {
            annotations: checker.annotations.clone(),
            final_initializers: checker.final_initializers.clone(),
            invalid_alias_uses: checker.invalid_alias_uses.clone(),
            expected_calls: checker.expected_calls.clone(),
            parameter_spec_errors: checker.parameter_spec_errors.clone(),
            partial_dict_lvalues: checker.partial_dict_lvalues.clone(),
            partial_list_lvalues: checker.partial_list_lvalues.clone(),
        }
    }

    pub(in crate::infrastructure::mypy::checker) fn restore(&self, checker: &mut Checker<'_>) {
        checker.annotations = self.annotations.clone();
        checker.final_initializers = self.final_initializers.clone();
        checker.invalid_alias_uses = self.invalid_alias_uses.clone();
        checker.expected_calls = self.expected_calls.clone();
        checker.parameter_spec_errors = self.parameter_spec_errors.clone();
        checker.partial_dict_lvalues = self.partial_dict_lvalues.clone();
        checker.partial_list_lvalues = self.partial_list_lvalues.clone();
    }
}

impl Checker<'_> {
    pub(super) fn loop_backedges<'a>(
        &mut self,
        body: &'a [Stmt],
        entry: Entry<'a>,
        location: TextRange,
        baseline: &BTreeMap<String, Type>,
    ) -> (LoopFrame, Flow)
    where
        Self: Visitor<'a>,
    {
        // This is an explicit resource limit, not a convergence rule. Reaching
        // it records a native incomplete-state gap and cannot certify a pass.
        const MAX_PASSES: usize = 64;
        let namespace = self.scopes.loop_namespace().to_owned();
        let _source = self.context.source_declarations(
            &namespace,
            &self.source.sha256,
            self.python_target,
            body,
            match &entry {
                Entry::While(test) => Some(*test),
                Entry::For { .. } => None,
            },
        );
        self.scopes.begin_loop_replay(location);
        let diagnostics = self.begin_iteration_diagnostics("Loop");
        let bookkeeping = Bookkeeping::save(self);
        let incoming = self.flow.clone();
        let mut head = incoming.clone();
        let mut last = LoopFrame::default();
        let mut exhausted = incoming.clone();
        let mut converged = false;
        for _ in 0..MAX_PASSES {
            let declarations = self.scopes.loop_state();
            let classes = self.context.loop_class_state(&namespace);
            let aliases = self.context.loop_alias_state(&namespace);
            let partials = self.partial_loop_state();
            let instances = self.instances.clone();
            let invalid_aliases = self.invalid_alias_annotations.clone();
            let pattern_bindings = self.untyped_pattern_bindings.clone();
            bookkeeping.restore(self);
            self.flow = head.clone();
            self.begin_iteration_context(diagnostics);
            match &entry {
                Entry::For {
                    target,
                    item,
                    invariant_columns,
                    start,
                } => self.bind_loop_target(target, *item, *start, invariant_columns),
                Entry::While(test) => self.visit_expr(test),
            }
            exhausted = self.flow.clone();
            self.loops.push(LoopFrame::default());
            if let Entry::While(test) = &entry {
                self.flow_condition(test, true);
            }
            self.visit_block(body);
            last = self.loops.pop().expect("loop body owns its jump frame");
            let mut backedges = vec![incoming.clone(), head.clone(), self.flow.clone()];
            backedges.extend(last.continues.iter().cloned());
            self.merge_flow(&backedges, baseline);
            let next = self.flow.clone();
            let (next_classes, intersection_gap) =
                self.context.reconcile_loop_class_caches(&classes);
            if intersection_gap {
                self.loop_gaps.insert(format!(
                    "Loop source class change needs predicate intersection replay in {namespace} at byte {}",
                    location.start().to_u32()
                ));
            }
            self.finish_iteration_context(diagnostics);
            if next == head
                && self.scopes.loop_state() == declarations
                && next_classes == classes
                && self.context.loop_alias_state(&namespace) == aliases
                && self.partial_loop_state() == partials
                && self.instances == instances
                && self.invalid_alias_annotations == invalid_aliases
                && self.untyped_pattern_bindings == pattern_bindings
            {
                converged = true;
                head = next;
                break;
            }
            head = next;
            self.scopes.repeat_loop_replay();
        }
        if !converged {
            self.loop_gaps.insert(format!(
                "Loop type flow did not converge in {namespace} at byte {} after {MAX_PASSES} passes",
                location.start().to_u32()
            ));
        }
        match entry {
            Entry::For { .. } => exhausted = head,
            Entry::While(test) => {
                self.flow = exhausted;
                self.flow_condition(test, false);
                exhausted = self.flow.clone();
            }
        }
        self.scopes.end_loop_replay();
        self.settle_iteration_diagnostics(diagnostics);
        (last, exhausted)
    }
}
