//! Exception inputs are separate from terminal routes and diagnostic settlement.
use super::*;
use types::Type;

mod for_targets;
mod member_writes;
mod with_items;

type ExecutionScope = (usize, scopes::Kind, Option<usize>);

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum PartialFamily {
    Set,
    List,
    Dict,
    Defaultdict,
}

#[derive(Clone, PartialEq, Eq)]
pub(super) struct DeclarationView {
    pub(super) name: String,
    binding_owner: usize,
    origin: ruff_text_size::TextSize,
    partial_owner: usize,
    family: PartialFamily,
    nominal: Option<u32>,
}

#[derive(Clone, PartialEq, Eq)]
enum InputValue {
    Concrete(Type),
    Lexical(DeclarationView),
}

pub(super) struct MaterializedInputs {
    pub(super) baseline: BTreeMap<String, Type>,
    pub(super) options: BTreeMap<String, Option<Vec<Type>>>,
    pub(super) deleted: BTreeSet<String>,
    pub(super) count: usize,
    pub(super) role_lost: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum InputKind {
    BodyHandlers,
    OuterFinally,
}

#[derive(Default)]
pub(super) struct TryInputs {
    frames: Vec<InputFrame>,
    undo: Vec<FrameUndo>,
    checkpoints: usize,
    writers: Vec<WriterVisit>,
    coverage: Vec<CoverageWatch>,
    member_inputs: member_writes::MemberInputs,
}

struct InputFrame {
    kind: InputKind,
    owner: ExecutionScope,
    inputs: CommitInputs,
    refused: bool,
    // Completion is checked only after the real traversal closes this frame.
    writer_depth: usize,
}

pub(super) struct InputCheckpoint {
    undo: usize,
    frames: usize,
    level: usize,
    writers: Vec<WriterVisit>,
    coverage: Vec<CoverageWatch>,
}

#[derive(Clone)]
struct CoverageWatch {
    owner: ExecutionScope,
    writer_depth: usize,
    refused: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum WriterDisposition {
    Unknown,
    BinderCommit,
    NoBinderEvent,
}

#[derive(Clone)]
struct WriterVisit {
    owner: ExecutionScope,
    statement: usize,
    failed: bool,
    target: Option<usize>,
    disposition: WriterDisposition,
    assignment: Option<AssignmentProof>,
    for_target: Option<for_targets::ForTarget>,
    member: Option<Box<member_writes::MemberWrite>>,
    with_items: Option<with_items::WithItems>,
}

#[derive(Clone)]
struct AssignmentProof {
    owner: ExecutionScope,
    origin: ruff_text_size::TextSize,
    binding: Option<usize>,
    definition: bool,
    checked: bool,
    declaration: Option<Type>,
}

struct FrameUndo {
    index: usize,
    count: usize,
    refused: bool,
    deleted: DeletedUndo,
    keys: Vec<KeyUndo>,
}

enum DeletedUndo {
    Unchanged,
    Initial(BTreeSet<String>),
    Removed(Vec<String>),
}

enum KeyUndo {
    Inserted(String),
    Missing(String, Vec<InputValue>),
    Appended(String, usize),
}

// One fixed entry baseline and ordered distinct options, not N Flow clones.
pub(super) struct CommitInputs {
    baseline: BTreeMap<String, InputValue>,
    options: BTreeMap<String, Option<Vec<InputValue>>>,
    pub(super) deleted: BTreeSet<String>,
    pub(super) count: usize,
}

impl CommitInputs {
    fn new(baseline: &BTreeMap<String, Type>, incoming: &flow::Flow) -> Self {
        let mut inputs = Self {
            baseline: baseline
                .iter()
                .map(|(name, &ty)| (name.clone(), Self::value(incoming, name, ty)))
                .collect(),
            options: BTreeMap::new(),
            deleted: BTreeSet::new(),
            count: 0,
        };
        let _ = inputs.push(incoming, false);
        inputs
    }

    fn push(&mut self, flow: &flow::Flow, journal: bool) -> (DeletedUndo, Vec<KeyUndo>) {
        let mut keys = Vec::new();
        if !flow.reachable {
            return (DeletedUndo::Unchanged, keys);
        }
        let deleted = if self.count == 0 {
            let old = std::mem::replace(&mut self.deleted, flow.deleted.clone());
            if journal {
                DeletedUndo::Initial(old)
            } else {
                DeletedUndo::Unchanged
            }
        } else {
            let mut removed = Vec::new();
            self.deleted.retain(|name| {
                let keep = flow.deleted.contains(name);
                if !keep && journal {
                    removed.push(name.clone());
                }
                keep
            });
            DeletedUndo::Removed(removed)
        };
        for (name, options) in &mut self.options {
            let ty = flow
                .narrowed
                .get(name)
                .map(|&ty| Self::value(flow, name, ty))
                .or_else(|| self.baseline.get(name).cloned());
            match (options.as_mut(), ty) {
                (Some(values), Some(ty)) => {
                    if !values.contains(&ty) {
                        if journal {
                            keys.push(KeyUndo::Appended(name.clone(), values.len()));
                        }
                        values.push(ty);
                    }
                }
                (Some(_), None) => {
                    let old = options.take().expect("present ordered options");
                    if journal {
                        keys.push(KeyUndo::Missing(name.clone(), old));
                    }
                }
                _ => {}
            }
        }
        for (name, &ty) in flow.narrowed.iter() {
            let ty = Self::value(flow, name, ty);
            if self.options.contains_key(name) {
                continue;
            }
            // All prior inputs lacked this key. Only the entry baseline can
            // supply those prior options. Missing never becomes Any.
            let mut options = if self.count == 0 {
                Some(Vec::new())
            } else {
                self.baseline.get(name).cloned().map(|ty| vec![ty])
            };
            if let Some(values) = &mut options {
                if !values.contains(&ty) {
                    values.push(ty);
                }
            }
            if journal {
                keys.push(KeyUndo::Inserted(name.clone()));
            }
            self.options.insert(name.clone(), options);
        }
        self.count += 1;
        (deleted, keys)
    }
    fn value(flow: &flow::Flow, name: &str, ty: Type) -> InputValue {
        flow.narrowed
            .view(name)
            .cloned()
            .map_or(InputValue::Concrete(ty), InputValue::Lexical)
    }
    fn materialize(self, checker: &Checker<'_>) -> Option<MaterializedInputs> {
        let role_lost = self
            .baseline
            .values()
            .any(|value| matches!(value, InputValue::Lexical(_)))
            || self
                .options
                .values()
                .filter_map(Option::as_ref)
                .flatten()
                .any(|value| matches!(value, InputValue::Lexical(_)));
        let resolve = |value| match value {
            InputValue::Concrete(ty) => Some(ty),
            InputValue::Lexical(view) => checker.partial_declaration_type(&view),
        };
        let baseline = self
            .baseline
            .into_iter()
            .map(|(name, value)| Some((name, resolve(value)?)))
            .collect::<Option<BTreeMap<_, _>>>()?;
        let options = self
            .options
            .into_iter()
            .map(|(name, values)| {
                Some((
                    name,
                    match values {
                        None => None,
                        Some(values) => Some(
                            values
                                .into_iter()
                                .map(resolve)
                                .collect::<Option<Vec<_>>>()?,
                        ),
                    },
                ))
            })
            .collect::<Option<BTreeMap<_, _>>>()?;
        Some(MaterializedInputs {
            baseline,
            options,
            deleted: self.deleted,
            count: self.count,
            role_lost,
        })
    }
}

impl TryInputs {
    pub(super) fn checkpoint(&mut self) -> InputCheckpoint {
        self.checkpoints += 1;
        InputCheckpoint {
            undo: self.undo.len(),
            frames: self.frames.len(),
            level: self.checkpoints,
            writers: self.writers.clone(),
            coverage: self.coverage.clone(),
        }
    }

    pub(super) fn commit(&mut self, point: InputCheckpoint) {
        if self.checkpoints != point.level
            || self.frames.len() != point.frames
            || self.writers.len() != point.writers.len()
            || self.coverage.len() != point.coverage.len()
        {
            self.refuse();
            return;
        }
        self.checkpoints -= 1;
        if self.checkpoints == 0 {
            self.undo.clear();
        }
    }

    pub(super) fn rollback(&mut self, point: InputCheckpoint) {
        if self.checkpoints != point.level
            || self.frames.len() != point.frames
            || self.writers.len() != point.writers.len()
            || self.coverage.len() != point.coverage.len()
        {
            self.refuse();
            return;
        }
        for change in self.undo.drain(point.undo..).rev() {
            let frame = &mut self.frames[change.index];
            frame.refused = change.refused;
            frame.inputs.count = change.count;
            match change.deleted {
                DeletedUndo::Unchanged => {}
                DeletedUndo::Initial(old) => frame.inputs.deleted = old,
                DeletedUndo::Removed(removed) => frame.inputs.deleted.extend(removed),
            }
            for key in change.keys.into_iter().rev() {
                match key {
                    KeyUndo::Inserted(name) => {
                        frame.inputs.options.remove(&name);
                    }
                    KeyUndo::Missing(name, old) => {
                        frame.inputs.options.insert(name, Some(old));
                    }
                    KeyUndo::Appended(name, len) => {
                        if let Some(Some(values)) = frame.inputs.options.get_mut(&name) {
                            values.truncate(len);
                        }
                    }
                }
            }
        }
        self.writers = point.writers;
        self.coverage = point.coverage;
        self.checkpoints -= 1;
        if self.checkpoints == 0 {
            self.undo.clear();
        }
    }

    fn refuse(&mut self) {
        for coverage in &mut self.coverage {
            coverage.refused = true;
        }
        for (index, frame) in self.frames.iter_mut().enumerate() {
            if self.checkpoints != 0 && !frame.refused {
                self.undo.push(FrameUndo {
                    index,
                    count: frame.inputs.count,
                    refused: false,
                    deleted: DeletedUndo::Unchanged,
                    keys: Vec::new(),
                });
            }
            frame.refused = true;
        }
    }
}

impl Checker<'_> {
    pub(super) fn exception_alias_inputs_missing(
        &self,
        name: &str,
        owner: usize,
        fresh: bool,
    ) -> bool {
        let current = self.scopes.try_input_scope();
        let member = format!("{name}.");
        let index = format!("{name}[");
        let dependent =
            |key: &str| key.starts_with(member.as_str()) || key.starts_with(index.as_str());
        self.try_inputs.checkpoints == 0
            && current.0 == owner
            && current.2.is_none()
            && !self.try_inputs.frames.is_empty()
            && !self.flow.narrowed.unproved()
            && self.flow.narrowed.view(name).is_none()
            && (!fresh
                || !self.flow.deleted.contains(name)
                    && !self
                        .flow
                        .narrowed
                        .keys()
                        .any(|key| key == name || dependent(key)))
            && self
                .try_inputs
                .coverage
                .iter()
                .all(|watch| !watch.refused && watch.owner == current)
            && self.try_inputs.frames.iter().all(|frame| {
                frame.owner == current
                    && !frame.refused
                    && frame.inputs.count != 0
                    && !frame
                        .inputs
                        .baseline
                        .keys()
                        .any(|key| key == name || dependent(key))
                    && frame
                        .inputs
                        .options
                        .iter()
                        .all(|(key, options)| !dependent(key) && (key != name || options.is_none()))
            })
    }
}

impl Checker<'_> {
    pub(super) fn begin_try_input(&mut self, kind: InputKind, baseline: &BTreeMap<String, Type>) {
        self.retire_try_member_inputs();
        if baseline
            .keys()
            .chain(self.flow.narrowed.keys())
            .any(|name| self.scopes.unproved_inferred_none_declaration(name))
        {
            self.refuse_local_none_inputs();
        }
        self.try_inputs.frames.push(InputFrame {
            kind,
            owner: self.scopes.try_input_scope(),
            inputs: CommitInputs::new(baseline, &self.flow),
            refused: self.flow.narrowed.unproved()
                || self
                    .flow
                    .narrowed
                    .keys()
                    .chain(self.flow.deleted.iter())
                    .any(|key| key.contains('\0'))
                || self
                    .try_inputs
                    .writers
                    .iter()
                    .any(|writer| writer.disposition == WriterDisposition::Unknown),
            writer_depth: self.try_inputs.writers.len(),
        });
    }

    pub(super) fn finish_try_input(
        &mut self,
        kind: InputKind,
        _qualified: bool,
    ) -> Option<MaterializedInputs> {
        self.check_try_member_inputs(None);
        let frame = self.try_inputs.frames.pop().expect("active try input");
        (frame.kind == kind
            && frame.owner == self.scopes.try_input_scope()
            && !frame.refused
            && !self.flow.narrowed.unproved()
            && self.try_inputs.writers.len() == frame.writer_depth)
            .then_some(frame.inputs)?
            .materialize(self)
    }

    pub(super) fn refuse_try_inputs(&mut self) {
        self.try_inputs.refuse();
    }

    pub(super) fn refuse_local_none_inputs(&mut self) {
        // Keep the unknown role with cloned/restored Flow even outside a try.
        self.flow.narrowed.refuse_declaration_view();
        self.refuse_try_inputs();
    }

    pub(super) fn record_try_input_commit(
        &mut self,
        target: &Expr,
        actual: Option<Type>,
        accepted: bool,
    ) {
        self.check_try_member_inputs(Some(target));
        // The target can be absent from narrowed Flow. Neither that absence nor
        // a readable None proves a material declaration or a pending partial.
        if matches!(target, Expr::Name(name)
            if self.scopes.unproved_inferred_none_declaration(name.id.as_str()))
            || !self.try_inputs.frames.is_empty()
                && self
                    .flow
                    .narrowed
                    .keys()
                    .any(|name| self.scopes.unproved_inferred_none_declaration(name))
        {
            self.refuse_local_none_inputs();
        }
        if self.try_inputs.frames.is_empty() {
            return;
        }
        let owner = self.scopes.try_input_scope();
        let local = matches!(target, Expr::Name(name)
            if self.scopes.loop_binding_owner(name.id.as_str()) == owner.0)
            || self.completed_try_member_writer(target);
        let safe = !self.flow.narrowed.unproved() && accepted && actual.is_some_and(|ty| ty != Type::Never)
            && local && owner.2.is_none()
            // NUL cannot occur in a Python identifier. Internal with-result
            // temporaries must not be delivered as a user execution state.
            && !self.flow.narrowed.keys().chain(self.flow.deleted.iter())
                .any(|key| key.contains('\0'));
        let journal = self.try_inputs.checkpoints != 0;
        for (index, frame) in self.try_inputs.frames.iter_mut().enumerate() {
            let count = frame.inputs.count;
            let refused = frame.refused;
            let (deleted, keys) = if safe && frame.owner == owner && !frame.refused {
                frame.inputs.push(&self.flow, journal)
            } else {
                frame.refused = true;
                (DeletedUndo::Unchanged, Vec::new())
            };
            if journal {
                self.try_inputs.undo.push(FrameUndo {
                    index,
                    count,
                    refused,
                    deleted,
                    keys,
                });
            }
        }
    }
    pub(super) fn check_chained_assignment_type(
        &mut self,
        expected: types::Type,
        actual: types::Type,
        start: ruff_text_size::TextSize,
    ) -> bool {
        let point = ruff_text_size::TextRange::empty(start);
        let first = self.diagnostics.len();
        let accepted = self.check_assignment_type(expected, actual, &point);
        for diagnostic in &mut self.diagnostics[first..] {
            // TempNode has a virtual point, not a source character span.
            diagnostic.location.end_line = Some(diagnostic.location.line);
            diagnostic.location.end_column = Some(i64::from(diagnostic.location.column));
        }
        accepted
    }
}

impl Checker<'_> {
    fn partial_view(
        &self,
        name: &str,
        origin: ruff_text_size::TextSize,
        family: PartialFamily,
        binding_owner: usize,
    ) -> Option<DeclarationView> {
        if self.scopes.inferred_binding_identity(name) != Some((binding_owner, origin)) {
            return None;
        }
        let (partial_owner, nominal) = if family == PartialFamily::Defaultdict {
            let partial = self.partial_defaultdicts.get(&origin)?;
            if partial.name != name || partial.binding_owner != binding_owner {
                return None;
            }
            (partial.owner, Some(partial.seed.class_id))
        } else {
            let partial = self.partial_sets.get(&origin)?;
            let kind = match family {
                PartialFamily::Set => types::Container::Set,
                PartialFamily::List => types::Container::List,
                PartialFamily::Dict => types::Container::Dict,
                PartialFamily::Defaultdict => return None,
            };
            if !partial.active
                || partial.name != name
                || partial.binding_owner != Some(binding_owner)
                || partial.kind != kind
                || family == PartialFamily::List && self.partial_list_unknown.contains(&origin)
            {
                return None;
            }
            (partial.owner, None)
        };
        Some(DeclarationView {
            name: name.into(),
            binding_owner,
            origin,
            partial_owner,
            family,
            nominal,
        })
    }

    fn partial_declaration_type(&self, view: &DeclarationView) -> Option<Type> {
        let current =
            self.partial_view(&view.name, view.origin, view.family, view.binding_owner)?;
        if current != *view || self.flow.deleted.contains(&view.name) {
            return None;
        }
        let inferred = if view.family == PartialFamily::Defaultdict {
            self.partial_defaultdicts.get(&view.origin)?.inferred?
        } else {
            self.partial_sets.get(&view.origin)?.inferred?
        };
        // Equality checks consistency with an already proved mutation, never origin.
        let declared =
            self.scopes
                .inferred_declaration(&view.name, view.binding_owner, view.origin)?;
        (declared == inferred).then_some(declared)
    }

    fn refuse_concrete_partial_history(&mut self, view: &DeclarationView) {
        if self.try_inputs.frames.iter().any(|frame| {
            matches!(
                frame.inputs.baseline.get(&view.name),
                Some(InputValue::Concrete(_))
            ) || frame
                .inputs
                .options
                .get(&view.name)
                .and_then(Option::as_ref)
                .is_some_and(|values| {
                    values
                        .iter()
                        .any(|value| matches!(value, InputValue::Concrete(_)))
                })
        }) {
            self.refuse_try_inputs();
        }
    }

    pub(super) fn write_partial_declaration(
        &mut self,
        name: &str,
        origin: ruff_text_size::TextSize,
        family: PartialFamily,
        receipt: (usize, Option<Type>, Type),
    ) {
        let view = self.partial_view(name, origin, family, receipt.0);
        if let Some(view) = &view {
            self.refuse_concrete_partial_history(view);
        }
        // This is the existing declaration-derived Flow write, not an assignment
        // exception option. The old Option type is retained in the actual receipt.
        self.flow
            .narrowed
            .declaration_write(name.into(), receipt.2, view);
    }

    pub(super) fn record_partial_initializer_commit(
        &mut self,
        target: &Expr,
        actual: Option<Type>,
        accepted: bool,
        family: Option<PartialFamily>,
    ) {
        let view = match (target, family) {
            (Expr::Name(name), Some(family))
                if accepted && actual.is_some_and(|ty| ty != Type::Never) =>
            {
                self.scopes
                    .inferred_binding_identity(name.id.as_str())
                    .filter(|(_, origin)| *origin == name.start())
                    .and_then(|(owner, origin)| {
                        self.partial_view(name.id.as_str(), origin, family, owner)
                    })
            }
            _ => None,
        };
        if let Some(view) = view {
            let pending = if view.family == PartialFamily::Defaultdict {
                self.partial_defaultdicts
                    .get(&view.origin)
                    .is_some_and(|partial| {
                        partial.active
                            && partial.inferred.is_none()
                            && partial.last_assignment == view.origin
                    })
            } else {
                self.partial_sets.get(&view.origin).is_some_and(|partial| {
                    partial.active
                        && partial.inferred.is_none()
                        && partial.last_assignment == view.origin
                })
            };
            if pending
                && view.partial_owner == self.partial_set_scope
                && self
                    .scopes
                    .inferred_declaration(&view.name, view.binding_owner, view.origin)
                    .is_some()
            {
                self.refuse_concrete_partial_history(&view);
                self.flow.narrowed.mark_view(view);
                // This is a real no-event branch, but materialization still must
                // prove the live declaration and may refuse the frame.
                self.complete_try_no_event(target);
                return;
            }
        }
        if self.complete_try_assignment(target, actual, accepted) {
            return;
        }
        self.record_try_input_commit(target, actual, accepted);
    }
}

impl Checker<'_> {
    pub(super) fn begin_try_writer(&mut self, statement: &Stmt) -> Option<usize> {
        if self.try_inputs.frames.is_empty() && self.try_inputs.coverage.is_empty() {
            return None;
        }
        let member = self.try_member_writer_seed(statement);
        let for_target = self.try_for_target_seed(statement);
        let with_items = self.try_with_items_seed(statement);
        let target = match statement {
            Stmt::Assign(assignment) => match assignment.targets.as_slice() {
                [target @ Expr::Name(_)] => Some(target as *const Expr as usize),
                [target @ Expr::Attribute(_)] if member.is_some() => {
                    Some(target as *const Expr as usize)
                }
                _ => None,
            },
            Stmt::AnnAssign(assignment) if assignment.target.is_name_expr() => {
                Some(assignment.target.as_ref() as *const Expr as usize)
            }
            Stmt::For(looped) if looped.target.is_name_expr() => {
                Some(looped.target.as_ref() as *const Expr as usize)
            }
            Stmt::For(looped) if for_target.is_some() => {
                Some(looped.target.as_ref() as *const Expr as usize)
            }
            Stmt::Import(_) | Stmt::ImportFrom(_) => None,
            Stmt::With(_) if with_items.is_some() => None,
            // These dispatches have no own binding write. Child expressions and
            // child statements still go through their actual writer hooks.
            Stmt::Expr(_)
            | Stmt::Assert(_)
            | Stmt::Return(_)
            | Stmt::Raise(_)
            | Stmt::If(_)
            | Stmt::While(_)
            | Stmt::Try(_)
            | Stmt::Pass(_)
            | Stmt::Break(_)
            | Stmt::Continue(_) => return None,
            _ => {
                self.refuse_try_inputs();
                return None;
            }
        };
        let depth = self.try_inputs.writers.len();
        self.try_inputs.writers.push(WriterVisit {
            owner: self.scopes.try_input_scope(),
            statement: statement as *const Stmt as usize,
            failed: false,
            target,
            disposition: WriterDisposition::Unknown,
            assignment: None,
            for_target,
            member,
            with_items,
        });
        Some(depth)
    }

    pub(super) fn finish_try_writer(&mut self, token: Option<usize>) {
        let Some(depth) = token else {
            return;
        };
        if self.try_inputs.writers.len() != depth + 1 {
            self.refuse_try_inputs();
            return;
        }
        let writer = self.try_inputs.writers.pop().expect("active writer visit");
        if writer.owner != self.scopes.try_input_scope()
            || writer.failed
            || writer.disposition == WriterDisposition::Unknown
        {
            self.refuse_try_inputs();
        }
    }

    pub(super) fn complete_try_import(&mut self, statement: &Stmt, proved: bool) {
        let Some(writer) = self.try_inputs.writers.last_mut() else {
            return;
        };
        if proved
            && writer.statement == statement as *const Stmt as usize
            && writer.owner == self.scopes.try_input_scope()
            && matches!(statement, Stmt::Import(_) | Stmt::ImportFrom(_))
        {
            writer.disposition = WriterDisposition::NoBinderEvent;
        }
    }

    pub(super) fn prepare_try_assignment(&mut self, target: &Expr, checked: Option<bool>) {
        self.prepare_try_member_writer(target, checked);
        if !self
            .try_inputs
            .writers
            .last()
            .is_some_and(|writer| writer.target == Some(target as *const Expr as usize))
        {
            return;
        }
        let Expr::Name(name) = target else {
            return;
        };
        let owner = self.scopes.try_input_scope();
        if owner.2.is_some() || self.scopes.loop_binding_owner(name.id.as_str()) != owner.0 {
            return;
        }
        let definition = self.scopes.inferred_binding_identity(name.id.as_str())
            == Some((owner.0, name.start()))
            || self.scopes.first_inferred_local_assignment(target) == Some((owner.0, name.start()));
        let Some(origin) = self
            .scopes
            .first_definition(name.id.as_str())
            .or_else(|| definition.then_some(name.start()))
        else {
            return;
        };
        let binding = self
            .scopes
            .resolved_binding(name.id.as_str())
            .map(|binding| binding as *const scopes::Binding as usize);
        self.try_inputs.writers.last_mut().unwrap().assignment = Some(AssignmentProof {
            owner,
            origin,
            binding,
            definition,
            checked: checked == Some(true),
            declaration: if definition {
                self.scopes
                    .inferred_declaration(name.id.as_str(), owner.0, origin)
            } else {
                self.scopes.assigned_type(target)
            },
        });
    }

    fn complete_try_assignment(
        &mut self,
        target: &Expr,
        actual: Option<Type>,
        accepted: bool,
    ) -> bool {
        self.complete_try_member_writer(target, actual, accepted);
        let Some(writer) = self.try_inputs.writers.last() else {
            return false;
        };
        if writer.target != Some(target as *const Expr as usize) {
            return false;
        }
        let Some(proof) = writer.assignment.clone() else {
            return false;
        };
        let Expr::Name(name) = target else {
            return false;
        };
        let live = self.scopes.try_input_scope() == proof.owner
            && self.scopes.loop_binding_owner(name.id.as_str()) == proof.owner.0
            && self.scopes.first_definition(name.id.as_str()) == Some(proof.origin)
            && if proof.definition {
                self.scopes.inferred_binding_identity(name.id.as_str())
                    == Some((proof.owner.0, proof.origin))
            } else {
                self.scopes
                    .resolved_binding(name.id.as_str())
                    .map(|binding| binding as *const scopes::Binding as usize)
                    == proof.binding
            };
        if !live
            || !accepted
            || !actual.is_some_and(|ty| ty != Type::Never)
            || self
                .scopes
                .unproved_inferred_none_declaration(name.id.as_str())
        {
            // Do not bypass the old record hook's sticky local-None refusal.
            return false;
        }
        let disposition = if proof.definition {
            if proof.declaration.is_none()
                || proof.declaration
                    != self.scopes.inferred_declaration(
                        name.id.as_str(),
                        proof.owner.0,
                        proof.origin,
                    )
            {
                return false;
            }
            WriterDisposition::NoBinderEvent
        } else if proof.checked {
            WriterDisposition::BinderCommit
        } else {
            WriterDisposition::Unknown
        };
        self.try_inputs.writers.last_mut().unwrap().disposition = disposition;
        disposition == WriterDisposition::NoBinderEvent
    }

    pub(super) fn complete_try_no_event(&mut self, target: &Expr) {
        let Some(writer) = self.try_inputs.writers.last() else {
            return;
        };
        let Some(proof) = writer
            .assignment
            .clone()
            .filter(|_| writer.target == Some(target as *const Expr as usize))
        else {
            return;
        };
        let Expr::Name(name) = target else {
            return;
        };
        let declared = if proof.definition {
            self.scopes
                .inferred_declaration(name.id.as_str(), proof.owner.0, proof.origin)
        } else {
            self.scopes.assigned_type(target)
        };
        if proof.owner == self.scopes.try_input_scope()
            && proof.owner.2.is_none()
            && self.scopes.loop_binding_owner(name.id.as_str()) == proof.owner.0
            && self.scopes.first_definition(name.id.as_str()) == Some(proof.origin)
            && self
                .scopes
                .resolved_binding(name.id.as_str())
                .map(|binding| binding as *const scopes::Binding as usize)
                == proof.binding
            && proof.declaration.is_some()
            && proof.declaration == declared
        {
            // A known no-event branch plus stable declaration evidence. Type
            // equality only checks consistency; it never creates a view role.
            self.try_inputs.writers.last_mut().unwrap().disposition =
                WriterDisposition::NoBinderEvent;
        }
    }

    pub(super) fn normal_finally_view_known(&self) -> bool {
        !self.flow.narrowed.unproved()
            && !self
                .flow
                .narrowed
                .keys()
                // A registrar may mark a view after flow_assignment removed the
                // first-definition value. Check those live record names too.
                .chain(self.partial_sets.values().map(|partial| &partial.name))
                .chain(
                    self.partial_defaultdicts
                        .values()
                        .map(|partial| &partial.name),
                )
                .any(|name| self.flow.narrowed.view(name).is_some())
    }
}

impl Checker<'_> {
    pub(super) fn begin_finally_coverage(&mut self) -> usize {
        self.retire_try_member_inputs();
        let depth = self.try_inputs.coverage.len();
        self.try_inputs.coverage.push(CoverageWatch {
            owner: self.scopes.try_input_scope(),
            writer_depth: self.try_inputs.writers.len(),
            refused: self.flow.narrowed.unproved(),
        });
        depth
    }

    pub(super) fn check_finally_coverage_state(&mut self) {
        if !self.check_try_member_inputs(None) || !self.normal_finally_view_known() {
            self.refuse_try_inputs();
        }
    }

    pub(super) fn finish_finally_coverage(&mut self, depth: usize) -> bool {
        self.check_try_member_inputs(None);
        if self.try_inputs.coverage.len() != depth + 1 {
            self.refuse_try_inputs();
            return false;
        }
        let coverage = self.try_inputs.coverage.pop().expect("finally coverage");
        let complete = !coverage.refused
            && coverage.owner == self.scopes.try_input_scope()
            && coverage.writer_depth == self.try_inputs.writers.len();
        if !complete {
            self.refuse_try_inputs();
        }
        complete
    }
}
