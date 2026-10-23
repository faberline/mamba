//! Exception aliases belong to the enclosing scope and are deleted after a handler.
use super::*;
use quality_syntax::python_ast::{ExceptHandler, StmtTry};
use std::collections::BTreeSet;
use types::{Node, Type};

#[derive(Clone, Copy, PartialEq, Eq)]
enum ExitKind {
    Return,
    Raise(Option<Type>),
}

#[derive(Clone, PartialEq, Eq)]
struct Exit {
    kind: ExitKind,
    flow: flow::Flow,
}

#[derive(Clone)]
pub(super) struct ExitFrame {
    function_depth: usize,
    class_depth: usize,
    exits: Vec<Exit>,
    observations: usize,
    ordinary_commits: Option<OrdinaryCommits>,
}

#[derive(Clone)]
struct OrdinaryCommit {
    name: String,
    owner: (usize, ruff_text_size::TextSize),
}

#[derive(Clone)]
struct OrdinaryCommits {
    // These addresses only match this visit's original targets, not semantic IDs.
    planned: BTreeMap<usize, OrdinaryCommit>,
    seen: BTreeSet<usize>,
    owners: BTreeMap<String, (usize, ruff_text_size::TextSize)>,
    reads: BTreeMap<String, Option<scopes::Binding>>,
    partial_scope: usize,
    invalid: bool,
}

struct OrdinaryExpressions {
    allowed: bool,
    names: BTreeMap<String, ruff_text_size::TextSize>,
}

impl<'a> Visitor<'a> for OrdinaryExpressions {
    fn visit_expr(&mut self, expression: &'a Expr) {
        if !self.allowed {
            return;
        }
        match expression {
            Expr::Name(name) => {
                self.names
                    .entry(name.id.to_string())
                    .and_modify(|first| *first = (*first).min(name.start()))
                    .or_insert(name.start());
            }
            Expr::BoolOp(_)
            | Expr::BinOp(_)
            | Expr::UnaryOp(_)
            | Expr::If(_)
            | Expr::Dict(_)
            | Expr::Set(_)
            | Expr::List(_)
            | Expr::Tuple(_)
            | Expr::Call(_)
            | Expr::Compare(_)
            | Expr::Attribute(_)
            | Expr::Subscript(_)
            | Expr::NumberLiteral(_)
            | Expr::BooleanLiteral(_)
            | Expr::NoneLiteral(_)
            | Expr::EllipsisLiteral(_)
            | Expr::StringLiteral(_)
            | Expr::BytesLiteral(_)
            | Expr::Slice(_)
            | Expr::Starred(_) => {}
            // Includes deferred scopes, comprehensions, walrus, suspension,
            // formatted strings and unknown expression forms.
            _ => {
                self.allowed = false;
                return;
            }
        }
        walk_expr(self, expression);
    }
}

#[derive(Clone, Copy)]
pub(super) struct OrdinaryAssignment {
    owner: (usize, ruff_text_size::TextSize),
    first: bool,
}

#[derive(Clone)]
enum Continuation {
    Normal,
    Break(usize),
    Continue(usize),
    Exit(ExitKind),
}

// This predicate is deliberately independent of Context and inferred types.
// These statements have no expressions. Their visitor paths only walk an empty
// leaf or record Flow/jump/terminal routes. Everything else stays legacy-only.
fn finally_replay_eligible(body: &[Stmt]) -> bool {
    body.iter().all(|statement| match statement {
        Stmt::Pass(_) | Stmt::Break(_) | Stmt::Continue(_) => true,
        Stmt::Raise(raised) => raised.exc.is_none() && raised.cause.is_none(),
        _ => false,
    })
}

impl Checker<'_> {
    fn ordinary_try_commits(
        &self,
        tried: &StmtTry,
        incoming: &flow::Flow,
    ) -> Option<OrdinaryCommits> {
        if !incoming.reachable
            || self.functions.is_empty()
            || !self.loops.is_empty()
            || !self.partial_sets.is_empty()
            || !self.partial_defaultdicts.is_empty()
        {
            return None;
        }
        let mut expressions = OrdinaryExpressions {
            allowed: true,
            names: BTreeMap::new(),
        };
        let mut planned = BTreeMap::new();
        let mut owners: BTreeMap<String, (usize, ruff_text_size::TextSize)> = BTreeMap::new();
        let mut first_writes = BTreeMap::new();
        for (index, statement) in tried.body.iter().enumerate() {
            match statement {
                Stmt::Assign(assignment) => {
                    let [target @ Expr::Name(name)] = assignment.targets.as_slice() else {
                        return None;
                    };
                    if name.id.as_str() == "_" {
                        return None;
                    }
                    let owner = if let Some(&owner) = owners.get(name.id.as_str()) {
                        owner
                    } else {
                        let owner = self
                            .scopes
                            .direct_inferred_local_binding(name.id.as_str())
                            .or_else(|| self.scopes.first_inferred_local_assignment(target))?;
                        owners.insert(name.id.to_string(), owner);
                        first_writes
                            .insert(name.id.to_string(), (target.start(), assignment.end()));
                        owner
                    };
                    if owners.values().any(|other| other.0 != owner.0) {
                        return None;
                    }
                    planned.insert(
                        target as *const _ as usize,
                        OrdinaryCommit {
                            name: name.id.to_string(),
                            owner,
                        },
                    );
                    expressions.visit_expr(&assignment.value);
                }
                Stmt::Expr(statement) => expressions.visit_expr(&statement.value),
                Stmt::Assert(statement) => {
                    expressions.visit_expr(&statement.test);
                    if let Some(message) = statement.msg.as_deref() {
                        expressions.visit_expr(message);
                    }
                }
                Stmt::Return(statement) if index + 1 == tried.body.len() => {
                    if let Some(value) = statement.value.as_deref() {
                        expressions.visit_expr(value);
                    }
                }
                Stmt::Raise(statement) if index + 1 == tried.body.len() => {
                    if let Some(value) = statement.exc.as_deref() {
                        expressions.visit_expr(value);
                    }
                    if let Some(value) = statement.cause.as_deref() {
                        expressions.visit_expr(value);
                    }
                }
                Stmt::Pass(_) => {}
                // No uncovered writers, speculative chain, nested try or scope.
                _ => return None,
            }
            if !expressions.allowed {
                return None;
            }
        }
        let &(scope, _) = owners.values().next()?;
        for key in incoming.narrowed.keys().chain(incoming.deleted.iter()) {
            expressions
                .names
                .entry(key.split(['.', '[']).next()?.into())
                .or_insert(ruff_text_size::TextSize::new(0));
        }
        let lexical = self.scopes.environment();
        let mut reads = BTreeMap::new();
        // A pre-existing unknown declaration is not a fresh first definition.
        for (name, owner) in &owners {
            if self.scopes.resolved_binding(name).is_some()
                && self.scopes.inferred_binding_identity(name) != Some(*owner)
            {
                return None;
            }
            if !lexical.contains_key(name) && owner.1 != first_writes[name].0 {
                return None;
            }
        }
        for name in expressions.names.keys().chain(owners.keys()) {
            if self.scopes.loop_binding_owner(name) != scope {
                return None;
            }
            // Inferred None can still be a pending partial declaration. This
            // source subset does not infer its lifecycle from a Flow overlay.
            if lexical.get(name) == Some(&Type::None)
                && self.scopes.inferred_binding_identity(name).is_some()
            {
                return None;
            }
            if !lexical.contains_key(name)
                && (owners.contains_key(name)
                    || self.scopes.inferred_binding_identity(name).is_some())
                && expressions
                    .names
                    .get(name)
                    .is_some_and(|read| first_writes.get(name).is_none_or(|(_, end)| read < end))
            {
                return None;
            }
            if !owners.contains_key(name) {
                reads.insert(name.clone(), self.scopes.resolved_binding(name).cloned());
            }
        }
        Some(OrdinaryCommits {
            planned,
            owners,
            reads,
            seen: BTreeSet::new(),
            partial_scope: self.partial_set_scope,
            invalid: false,
        })
    }

    pub(super) fn ordinary_try_assignment(&self, target: &Expr) -> Option<OrdinaryAssignment> {
        if !self
            .try_exits
            .iter()
            .any(|frame| frame.ordinary_commits.is_some())
        {
            return None;
        }
        let Expr::Name(name) = target else {
            return None;
        };
        let owner = self
            .scopes
            .direct_inferred_local_binding(name.id.as_str())
            .or_else(|| self.scopes.first_inferred_local_assignment(target))?;
        Some(OrdinaryAssignment {
            owner,
            first: self.scopes.first_inferred_local_assignment(target) == Some(owner),
        })
    }

    pub(super) fn record_ordinary_try_commit(
        &mut self,
        target: &Expr,
        actual: Option<Type>,
        before: Option<OrdinaryAssignment>,
        compatibility: Option<bool>,
    ) {
        if !self
            .try_exits
            .iter()
            .any(|frame| frame.ordinary_commits.is_some())
        {
            return;
        }
        let owner = match target {
            Expr::Name(name) => self.scopes.direct_inferred_local_binding(name.id.as_str()),
            _ => None,
        };
        let material = actual.is_some_and(|ty| {
            ty != Type::Never
                && !self
                    .context
                    .node(ty)
                    .is_some_and(|node| matches!(node.as_ref(), Node::Deleted(_)))
        });
        let partial_free = self.partial_sets.is_empty() && self.partial_defaultdicts.is_empty();
        for frame in &mut self.try_exits {
            let Some(commits) = &mut frame.ordinary_commits else {
                continue;
            };
            if frame.function_depth != self.functions.len()
                || frame.class_depth != self.classes.len()
            {
                commits.invalid = true;
                continue;
            }
            let address = target as *const _ as usize;
            let Some(planned) = commits.planned.get(&address) else {
                commits.invalid = true;
                continue;
            };
            let accepted = before.is_some_and(|before| {
                before.owner == planned.owner
                    && match compatibility {
                        Some(accepted) => accepted,
                        None => before.first && actual != Some(Type::None),
                    }
            });
            if commits.invalid
                || !self.flow.reachable
                || !partial_free
                || !material
                || !accepted
                || commits.partial_scope != self.partial_set_scope
                || owner != Some(planned.owner)
                || !matches!(target, Expr::Name(name) if name.id.as_str() == planned.name.as_str())
                || self
                    .flow
                    .narrowed
                    .keys()
                    .chain(self.flow.deleted.iter())
                    .any(|key| {
                        let root = key.split(['.', '[']).next().unwrap_or("");
                        !commits.owners.contains_key(root) && !commits.reads.contains_key(root)
                    })
                || !commits.seen.insert(address)
            {
                commits.invalid = true;
                continue;
            }
            // The shared transport samples the completed post-Any commit once.
            // This frame validates the unchanged bounded consumer proof.
        }
    }

    fn completed_ordinary_try_inputs(&self, commits: OrdinaryCommits) -> bool {
        if commits.invalid
            || commits.seen.len() != commits.planned.len()
            || commits.partial_scope != self.partial_set_scope
            || !self.partial_sets.is_empty()
            || !self.partial_defaultdicts.is_empty()
            || commits.owners.iter().any(|(name, &owner)| {
                self.scopes.direct_inferred_local_binding(name) != Some(owner)
            })
            || commits
                .reads
                .iter()
                .any(|(name, binding)| self.scopes.resolved_binding(name) != binding.as_ref())
        {
            return false;
        }
        true
    }

    fn begin_try_exits(&mut self) {
        self.try_exits.push(ExitFrame {
            function_depth: self.functions.len(),
            class_depth: self.classes.len(),
            exits: Vec::new(),
            observations: 0,
            ordinary_commits: None,
        });
    }

    fn record_try_exit(&mut self, kind: ExitKind, flow: flow::Flow) {
        if let Some(frame) = self.try_exits.last_mut().filter(|frame| {
            frame.function_depth == self.functions.len() && frame.class_depth == self.classes.len()
        }) {
            frame.observations += 1;
            let exit = Exit { kind, flow };
            if !frame.exits.contains(&exit) {
                frame.exits.push(exit);
            }
        }
    }

    pub(super) fn call_exit_marker(&self) -> Option<(usize, usize)> {
        self.try_exits
            .last()
            .filter(|frame| {
                frame.function_depth == self.functions.len()
                    && frame.class_depth == self.classes.len()
            })
            .map(|frame| (self.try_exits.len() - 1, frame.observations))
    }

    pub(super) fn record_nonreturning_call(&mut self, marker: Option<(usize, usize)>) {
        if let Some((index, observations)) = marker {
            if self
                .try_exits
                .get(index)
                .is_some_and(|frame| frame.observations == observations)
            {
                let mut flow = self.flow.clone();
                flow.reachable = true;
                self.record_try_exit(ExitKind::Raise(None), flow);
            }
        }
    }

    pub(super) fn record_failed_assert(&mut self, before: flow::Flow) {
        if before.reachable && !self.flow.reachable {
            let assertion = self
                .context
                .runtime_builtin_class_instance("AssertionError", self.python_target);
            self.record_try_exit(ExitKind::Raise(assertion), before);
        }
    }

    pub(super) fn record_return_exit(&mut self) {
        if self.flow.reachable {
            self.record_try_exit(ExitKind::Return, self.flow.clone());
        }
    }

    pub(super) fn record_raise_exit(&mut self, raised: Option<&Expr>) {
        if self.flow.reachable {
            let ty = raised.and_then(|raised| self.infer(raised)).and_then(|ty| {
                if self.context.is_class_value(ty) {
                    self.context.instance_type(ty)
                } else {
                    Some(ty)
                }
            });
            self.record_try_exit(ExitKind::Raise(ty), self.flow.clone());
        }
    }

    fn caught_try_raise(&self, raised: Option<Type>, tried: &StmtTry) -> bool {
        tried.handlers.iter().any(|handler| {
            let ExceptHandler::ExceptHandler(handler) = handler;
            let Some(test) = handler.type_.as_deref() else {
                return !tried.is_star;
            };
            let Some(actual) =
                raised.filter(|ty| !matches!(ty, Type::Any | Type::AnySpecial | Type::Never))
            else {
                return false;
            };
            self.infer(test)
                .and_then(|ty| self.exception_instance_type(ty, tried.is_star))
                .is_some_and(|expected| {
                    !expected.is_any() && self.context.accepts(expected, actual)
                })
        })
    }

    pub(super) fn check_try<'a>(&mut self, tried: &'a StmtTry)
    where
        Self: Visitor<'a>,
    {
        let baseline = self.environment();
        let incoming = self.flow.clone();
        let jump_prefix: Vec<_> = self
            .loops
            .iter()
            .map(|frame| (frame.breaks.len(), frame.continues.len()))
            .collect();
        let ordinary_commits = self.ordinary_try_commits(tried, &incoming);
        if !tried.finalbody.is_empty() {
            self.begin_try_input(try_inputs::InputKind::OuterFinally, &baseline);
        }
        self.begin_try_input(try_inputs::InputKind::BodyHandlers, &baseline);
        self.begin_try_exits();
        self.try_exits.last_mut().unwrap().ordinary_commits = ordinary_commits;
        self.visit_block(&tried.body);
        let body_frame = self.try_exits.pop().expect("try body exit frame");
        let qualified = body_frame
            .ordinary_commits
            .is_some_and(|commits| self.completed_ordinary_try_inputs(commits));
        let ordinary_inputs = self.finish_try_input(try_inputs::InputKind::BodyHandlers, qualified);
        if ordinary_inputs.is_none() {
            // Approximate handler input cannot later qualify an outer frame.
            self.refuse_try_inputs();
        }
        let body_exits = body_frame.exits;
        let mut exits: Vec<_> = body_exits
            .into_iter()
            .filter(|exit| match exit.kind {
                ExitKind::Return => true,
                ExitKind::Raise(raised) => !self.caught_try_raise(raised, tried),
            })
            .collect();
        self.begin_try_exits();
        let mut normal = self.flow.clone();
        // An exception can leave the body before its normal terminal statement.
        let mut exceptional = normal.clone();
        exceptional.reachable = incoming.reachable;
        if let Some(inputs) = ordinary_inputs {
            self.merge_commit_inputs(inputs);
        } else {
            self.merge_flow(&[incoming.clone(), exceptional], &baseline);
        }
        let mut handler_input = self.flow.clone();
        let mut outcomes: Vec<flow::Flow> = Vec::new();
        for handler in &tried.handlers {
            let ExceptHandler::ExceptHandler(handler) = handler;
            let handler_jumps: Vec<_> = self
                .loops
                .iter()
                .map(|frame| (frame.breaks.len(), frame.continues.len()))
                .collect();
            self.begin_try_exits();
            self.flow = handler_input.clone();
            if let Some(test) = handler.type_.as_deref() {
                self.visit_expr(test);
            }
            let mut alias_lifecycle = None;
            if let Some(name) = &handler.name {
                let ty = handler.type_.as_deref().and_then(|test| {
                    self.infer(test)
                        .and_then(|ty| self.exception_instance_type(ty, tried.is_star))
                });
                // Only a fresh local alias with no typed saved slot needs no
                // saved-option cleanse. Missing options remain Missing.
                let candidate = ty
                    .filter(|&ty| {
                        !tried.is_star
                            && self
                                .context
                                .node(ty)
                                .is_some_and(|node| matches!(node.as_ref(), Node::Instance(_)))
                    })
                    .and_then(|_| {
                        self.scopes
                            .fresh_exception_alias(name.id.as_str(), name.start())
                    })
                    .filter(|&owner| {
                        self.exception_alias_inputs_missing(name.id.as_str(), owner, true)
                    });
                if candidate.is_none() {
                    self.refuse_try_inputs();
                }
                let changed = self.scopes.set_exception_alias(name.id.as_str(), ty);
                if let Some(owner) = candidate {
                    if changed == Some((owner, name.start(), None, ty, true)) {
                        alias_lifecycle = ty.map(|ty| (owner, name.start(), ty));
                    } else {
                        self.refuse_try_inputs();
                    }
                }
                self.flow.delete_exception_alias(name.id.as_str());
                self.flow.deleted.remove(name.id.as_str());
                if let Some(ty) = ty {
                    self.flow.narrowed.insert(name.id.to_string(), ty);
                }
            }
            self.visit_block(&handler.body);
            if let Some(name) = &handler.name {
                let deleted = self.context.intern(Node::Deleted(name.id.to_string()));
                let changed = self
                    .scopes
                    .set_exception_alias(name.id.as_str(), Some(deleted));
                if let Some((owner, definition, entered)) = alias_lifecycle {
                    if changed != Some((owner, definition, Some(entered), Some(deleted), true))
                        || !self.exception_alias_inputs_missing(name.id.as_str(), owner, false)
                    {
                        self.refuse_try_inputs();
                    }
                }
                // Mypy deletes the declaration even when this handler raises.
                // Remove stale refinements from every pending fallthrough path.
                self.flow.delete_exception_alias(name.id.as_str());
                normal.delete_exception_alias(name.id.as_str());
                handler_input.delete_exception_alias(name.id.as_str());
                for outcome in &mut outcomes {
                    outcome.delete_exception_alias(name.id.as_str());
                }
                // The handler alias is deleted on terminal routes as well.
                for (frame, &(breaks, continues)) in self.loops.iter_mut().zip(&handler_jumps) {
                    for pending in frame.breaks[breaks..]
                        .iter_mut()
                        .chain(frame.continues[continues..].iter_mut())
                    {
                        pending.delete_exception_alias(name.id.as_str());
                    }
                }
                for pending in &mut self.try_exits.last_mut().unwrap().exits {
                    pending.flow.delete_exception_alias(name.id.as_str());
                }
            }
            let handler_exits = self.try_exits.pop().expect("handler terminal routes").exits;
            for exit in handler_exits {
                self.record_try_exit(exit.kind, exit.flow);
            }
            outcomes.push(self.flow.clone());
        }
        self.flow = normal;
        self.visit_block(&tried.orelse);
        outcomes.push(self.flow.clone());
        self.merge_flow(&outcomes, &baseline);
        exits.extend(
            self.try_exits
                .pop()
                .expect("handler and else exit frame")
                .exits,
        );
        if !tried.finalbody.is_empty() {
            // The own input frame stops before finally; enclosing frames stay live.
            let inputs = self.finish_try_input(try_inputs::InputKind::OuterFinally, false);
            let normal = self.flow.clone();
            let namespace = self.scopes.loop_namespace().to_owned();
            let _source = self.context.source_declarations(
                &namespace,
                &self.source.sha256,
                self.python_target,
                &tried.finalbody,
                None,
            );
            let location = ruff_text_size::TextRange::new(
                tried.finalbody.first().unwrap().start(),
                tried.finalbody.last().unwrap().end(),
            );
            if let Some(inputs) =
                inputs.filter(|inputs| inputs.count != 0 && self.normal_finally_view_known())
            {
                self.check_finally_contexts(
                    &tried.finalbody,
                    normal,
                    inputs,
                    exits,
                    &namespace,
                    location,
                );
                return;
            }
            let has_jump =
                self.loops
                    .iter()
                    .zip(&jump_prefix)
                    .any(|(frame, &(breaks, continues))| {
                        frame.breaks.len() > breaks || frame.continues.len() > continues
                    });
            let modeled = normal.reachable || !exits.is_empty() || has_jump;
            if !finally_replay_eligible(&tried.finalbody) || !modeled {
                self.loop_gaps.insert(format!(
                    "Finally body has no proved continuation replay in {namespace} at byte {}",
                    location.start().to_u32()
                ));
                // Select one coherent legacy analysis before any route fork.
                // No rejected route ever mutates a shared declaration registry.
                self.check_finally_legacy(
                    &tried.finalbody,
                    normal,
                    handler_input,
                    exits,
                    &jump_prefix,
                );
                return;
            }
            let mut paths = Vec::new();
            if normal.reachable {
                paths.push((normal, Continuation::Normal));
            }
            for (index, &(breaks, continues)) in jump_prefix.iter().enumerate() {
                paths.extend(
                    self.loops[index]
                        .breaks
                        .drain(breaks..)
                        .map(|flow| (flow, Continuation::Break(index))),
                );
                paths.extend(
                    self.loops[index]
                        .continues
                        .drain(continues..)
                        .map(|flow| (flow, Continuation::Continue(index))),
                );
            }
            paths.extend(
                exits
                    .into_iter()
                    .map(|exit| (exit.flow, Continuation::Exit(exit.kind))),
            );
            let mut normal_outcomes = Vec::new();
            for (input, continuation) in paths {
                // Eligibility proves Scope/partial transfer is the identity and
                // no Context class/alias mutation path can run. Fork only Flow.
                self.flow = input;
                self.visit_block(&tried.finalbody);
                if self.flow.reachable {
                    match continuation {
                        Continuation::Normal => normal_outcomes.push(self.flow.clone()),
                        Continuation::Break(index) => {
                            self.loops[index].breaks.push(self.flow.clone())
                        }
                        Continuation::Continue(index) => {
                            self.loops[index].continues.push(self.flow.clone())
                        }
                        Continuation::Exit(kind) => self.record_try_exit(kind, self.flow.clone()),
                    }
                }
                // A finally-owned terminal route replaces the old continuation.
            }
            self.merge_flow(&normal_outcomes, &baseline);
        } else {
            for exit in exits {
                self.record_try_exit(exit.kind, exit.flow);
            }
        }
    }

    fn check_finally_contexts<'a>(
        &mut self,
        body: &'a [Stmt],
        normal: flow::Flow,
        inputs: try_inputs::MaterializedInputs,
        exits: Vec<Exit>,
        namespace: &str,
        location: ruff_text_size::TextRange,
    ) where
        Self: Visitor<'a>,
    {
        let bookkeeping = loops::iteration::Bookkeeping::save(self);
        let diagnostics = self.begin_iteration_diagnostics("Finally");
        self.merge_commit_inputs(inputs);
        let coverage = self.begin_finally_coverage();
        self.begin_iteration_context(diagnostics);
        self.visit_block(body);
        self.finish_iteration_context(diagnostics);
        self.check_finally_coverage_state();
        if self.flow.reachable {
            // The saved kinds resume after this one aggregate abnormal context.
            // Their pre-finally Flow is not another binder input or visitor pass.
            // Finally-owned exits/jumps already reached their enclosing frames.
            for exit in exits {
                self.record_try_exit(exit.kind, self.flow.clone());
            }
        }
        // Mypy's fall_through=0 discards this ordinary Flow, not shared
        // declarations or jumps already sent to their enclosing target frames.
        self.flow = normal;
        if self.flow.reachable {
            bookkeeping.restore(self);
            self.begin_iteration_context(diagnostics);
            self.visit_block(body);
            self.finish_iteration_context(diagnostics);
            self.check_finally_coverage_state();
        }
        let complete = self.finish_finally_coverage(coverage);
        self.settle_iteration_diagnostics(diagnostics);
        if !complete {
            // A real visit cannot be undone as a Scopes/Context transaction.
            // Keep its result and the same public obligation; never rerun legacy.
            self.loop_gaps.insert(format!(
                "Finally body has no proved continuation replay in {namespace} at byte {}",
                location.start().to_u32()
            ));
        }
    }

    fn check_finally_legacy<'a>(
        &mut self,
        body: &'a [Stmt],
        normal: flow::Flow,
        handler_input: flow::Flow,
        exits: Vec<Exit>,
        prefixes: &[(usize, usize)],
    ) where
        Self: Visitor<'a>,
    {
        // Record the end of the body-owned suffix before finally can add routes.
        // A prefix predates this try; a suffix after these ends belongs to finally.
        let body_ends: Vec<_> = self
            .loops
            .iter()
            .map(|frame| (frame.breaks.len(), frame.continues.len()))
            .collect();
        let can_continue = normal.reachable;
        self.flow = if can_continue { normal } else { handler_input };
        self.visit_block(body);
        // Save raw fallthrough before applying the old post-try reachability mask.
        let falls_through = self.flow.reachable;
        if falls_through {
            for exit in exits {
                self.record_try_exit(exit.kind, exit.flow);
            }
        } else {
            // Remove only the original body continuations. Finally-owned routes
            // were appended after body_ends and survive alongside the old prefix.
            for ((frame, &(breaks, continues)), &(body_breaks, body_continues)) in
                self.loops.iter_mut().zip(prefixes).zip(&body_ends)
            {
                drop(frame.breaks.drain(breaks..body_breaks));
                drop(frame.continues.drain(continues..body_continues));
            }
        }
        self.flow.reachable &= can_continue;
    }

    fn exception_instance_type(&self, ty: Type, is_star: bool) -> Option<Type> {
        fn instance(context: &types::Context, ty: Type) -> Option<Type> {
            if matches!(ty, Type::Any | Type::AnySpecial | Type::Never) {
                return Some(ty);
            }
            match context.node(ty).as_deref() {
                Some(Node::Tuple(items) | Node::Union(items)) => {
                    let items = items
                        .iter()
                        .map(|&item| instance(context, item))
                        .collect::<Option<Vec<_>>>()?;
                    Some(context.union(items))
                }
                Some(Node::VariadicTuple(item)) => instance(context, *item),
                _ if context.is_class_value(ty) => context.instance_type(ty),
                _ => None,
            }
        }
        let item = instance(&self.context, ty)?;
        if !is_star {
            return Some(item);
        }
        let exception = self
            .context
            .runtime_builtin_class_instance("Exception", self.python_target)?;
        let name = if self.context.accepts(exception, item) {
            "ExceptionGroup"
        } else {
            "BaseExceptionGroup"
        };
        let group = self
            .context
            .runtime_builtin_class_instance(name, self.python_target)?;
        Some(
            self.context
                .applied_class(self.context.class_id(group)?, vec![item], false),
        )
    }
}
