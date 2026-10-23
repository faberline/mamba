//! Empty builtin sets retain element constraints and their original error positions.
use super::*;
use quality_syntax::python_ast::{ExprCall, ExprName, StmtAssign};
use ruff_text_size::TextSize;
use types::{Container, Node, Type};

#[derive(Clone)]
pub(super) struct PartialSet {
    pub(super) name: String,
    pub(super) target: Expr,
    pub(super) inferred: Option<Type>,
    pub(super) active: bool,
    pub(super) last_assignment: TextSize,
    pub(super) preserve_error: bool,
    pub(super) diagnostic: usize,
    pub(super) owner: usize,
    pub(super) kind: Container,
    pub(super) allow_unresolved_exit: bool,
    pub(super) binding_owner: Option<usize>,
}

// This is a normal read of a live unresolved declaration whose origin error
// is already committed. It is recovery evidence, never inferred type evidence.
#[derive(Clone)]
pub(super) struct PartialReadRecovery {
    pub(super) name: String,
    pub(super) binding_owner: usize,
    pub(super) partial_owner: usize,
    pub(super) origin: TextSize,
    pub(super) declaration: ruff_text_size::TextRange,
    pub(super) declaration_type: Type,
    pub(super) read: ruff_text_size::TextRange,
    pub(super) diagnostic: usize,
    pub(super) actual: Type,
}

// Diagnostic indices are storage locations, not semantic convergence inputs.
#[derive(PartialEq, Eq)]
pub(super) struct LoopState {
    containers: Vec<LoopContainer>,
    defaultdicts: Vec<partial_defaultdicts::SemanticState>,
    defaultdict_gaps: BTreeSet<String>,
    list_unknown: BTreeSet<TextSize>,
    list_gaps: BTreeSet<String>,
}

#[derive(PartialEq, Eq)]
struct LoopContainer {
    origin: TextSize,
    name: String,
    target: ruff_text_size::TextRange,
    inferred: Option<Type>,
    active: bool,
    last_assignment: TextSize,
    preserve_error: bool,
    owner: usize,
    kind: Container,
    allow_unresolved_exit: bool,
    binding_owner: Option<usize>,
}

impl Checker<'_> {
    pub(in crate::infrastructure::mypy::checker) fn partial_loop_state(&self) -> LoopState {
        LoopState {
            containers: self
                .partial_sets
                .iter()
                .map(|(&origin, partial)| LoopContainer {
                    origin,
                    name: partial.name.clone(),
                    target: partial.target.range(),
                    inferred: partial.inferred,
                    active: partial.active,
                    last_assignment: partial.last_assignment,
                    preserve_error: partial.preserve_error,
                    owner: partial.owner,
                    kind: partial.kind,
                    allow_unresolved_exit: partial.allow_unresolved_exit,
                    binding_owner: partial.binding_owner,
                })
                .collect(),
            defaultdicts: self.partial_defaultdict_state(),
            defaultdict_gaps: self.partial_defaultdict_gaps.clone(),
            list_unknown: self.partial_list_unknown.clone(),
            list_gaps: self.partial_list_gaps.clone(),
        }
    }

    pub(super) fn enter_partial_set_scope(&mut self) -> usize {
        let previous = self.partial_set_scope;
        self.partial_set_scope = self.next_partial_set_scope;
        self.next_partial_set_scope += 1;
        previous
    }

    fn partial_set_key(&self, name: &str) -> Option<TextSize> {
        let definition = self.scopes.binding_location(name)?;
        self.partial_sets
            .get(&definition)
            .filter(|partial| {
                partial.active && partial.name == name && partial.kind == Container::Set
            })
            .map(|_| definition)
    }

    pub(super) fn remember_partial_set(
        &mut self,
        assignment: &StmtAssign,
        incomplete: Option<Container>,
    ) -> bool {
        let [target] = assignment.targets.as_slice() else {
            return false;
        };
        self.remember_partial_set_initializer(target, &assignment.value, incomplete)
    }

    pub(super) fn remember_literal_partial_set(
        &mut self,
        target: &Expr,
        value: &Expr,
        actual: Option<Type>,
        first_inference: bool,
    ) -> (Option<Type>, Option<try_inputs::PartialFamily>) {
        let (Expr::Name(_), Expr::Call(call)) = (target, value) else {
            return (actual, None);
        };
        if !call.arguments.args.is_empty() || !call.arguments.keywords.is_empty() {
            return (actual, None);
        }
        let incomplete =
            self.scopes
                .incomplete_collection_constructor(value, actual, &self.environment());
        let registered = self.remember_partial_set_initializer(target, value, incomplete);
        if first_inference && registered && incomplete == Some(Container::Set) {
            // Use the same proven empty-constructor assignment recovery as
            // the ordinary whole-call path, without using Any as evidence.
            (
                actual.map(|ty| {
                    self.context
                        .substitute(ty, &BTreeMap::from([(Type::Never, Type::Any)]))
                }),
                Some(try_inputs::PartialFamily::Set),
            )
        } else {
            (actual, None)
        }
    }

    fn remember_partial_set_initializer(
        &mut self,
        target: &Expr,
        value: &Expr,
        incomplete: Option<Container>,
    ) -> bool {
        let (target @ Expr::Name(name), Expr::Call(call)) = (target, value) else {
            return false;
        };
        if !call.arguments.args.is_empty() || !call.arguments.keywords.is_empty() {
            return false;
        }
        let Some(constructor) = self
            .scopes
            .callable_expression_in(&call.func, &self.environment())
        else {
            return false;
        };
        if !constructor.constructor
            || constructor.name.as_deref() != Some("set")
            || constructor.definition_module.as_deref() != Some("builtins")
        {
            return false;
        }
        if let Some(key) = self.partial_set_key(name.id.as_str()) {
            let replay = key == name.start()
                && self.partial_sets[&key].owner == self.partial_set_scope
                && self.partial_sets[&key].binding_owner
                    == Some(self.scopes.loop_binding_owner(name.id.as_str()))
                && self
                    .scopes
                    .inferred_binding_identity(name.id.as_str())
                    .is_some_and(|(_, first_definition)| first_definition == key)
                && self.scopes.replayed_loop_definition(name.id.as_str(), key)
                && self.context.source_declaration_site(name);
            if replay {
                // Revisit the same source initializer for this live lexical
                // owner. Keep its slot, inferred constraint and read commitment.
                return true;
            }
            let partial = self.partial_sets.get_mut(&key).unwrap();
            if partial.inferred.is_none() {
                // A new empty initializer does not erase the unresolved earlier assignment.
                partial.preserve_error = true;
                partial.last_assignment = name.start();
                return true;
            }
        }
        // A compound suite can reach this initializer before normal assignment binds it.
        // Reject an existing different binding; later refinement still requires this offset.
        if incomplete != Some(Container::Set)
            || self
                .scopes
                .binding_location(name.id.as_str())
                .is_some_and(|definition| definition != name.start())
            || self.scopes.assigned_type(target).is_some()
        {
            return false;
        }
        let diagnostic = self.diagnostics.len();
        // Reserve the original position without consuming an ignore for a provisional error.
        self.diagnostics
            .push(crate::infrastructure::diagnostics::at(
                self.source,
                QualityTool::Mypy,
                "var-annotated",
                format!(
                    "Need type annotation for \"{}\" (hint: \"{}: set[<type>] = ...\")",
                    name.id, name.id
                ),
                target.range(),
            ));
        self.partial_sets.insert(
            name.start(),
            PartialSet {
                name: name.id.to_string(),
                target: target.clone(),
                inferred: None,
                active: true,
                last_assignment: name.start(),
                preserve_error: false,
                diagnostic,
                owner: self.partial_set_scope,
                kind: Container::Set,
                allow_unresolved_exit: false,
                binding_owner: Some(self.scopes.loop_binding_owner(name.id.as_str())),
            },
        );
        true
    }

    pub(super) fn check_partial_set_add(&mut self, call: &ExprCall) -> bool {
        let Expr::Attribute(attribute) = call.func.as_ref() else {
            return false;
        };
        let Expr::Name(receiver) = attribute.value.as_ref() else {
            return false;
        };
        if attribute.attr.as_str() != "add"
            || self.none_allowed != Some(call.range())
            || !call.arguments.keywords.is_empty()
            || self.flow.deleted.contains(receiver.id.as_str())
        {
            return false;
        }
        let [argument] = call.arguments.args.as_ref() else {
            return false;
        };
        if argument.is_starred_expr() {
            return false;
        }
        let Some(key) = self.partial_set_key(receiver.id.as_str()) else {
            return false;
        };
        let inferred = self.partial_sets[&key].inferred;
        let Some(constructor) = self
            .context
            .collection_constructor("set", self.python_target)
        else {
            return false;
        };
        let constructor = self.context.callable_type(constructor);
        let Some(method) =
            self.context
                .builtin_constructor_member(constructor, "add", self.python_target)
        else {
            return false;
        };
        if method.definition_module.as_deref() != Some("builtins")
            || method.owner.as_deref() != Some("set")
        {
            return false;
        }
        let ty = if let Some(inferred) = inferred {
            inferred
        } else {
            let environment = self
                .scopes
                .inference_environment(argument, self.environment());
            let Some(item) = self
                .context
                .infer_partial_set_item(argument, &environment, &|value| {
                    self.scopes.expression_type(value, &environment)
                })
                .or_else(|| self.untyped_pattern_value(argument))
                .filter(|&ty| ty.is_any() || self.known_set_item(ty))
            else {
                return false;
            };
            self.context
                .intern(Node::Container(Container::Set, vec![item]))
        };
        let Some(receiver_type) = method.parameters.first().and_then(|parameter| parameter.ty)
        else {
            return false;
        };
        let solution = self.context.solve(&method.generic, &[(receiver_type, ty)]);
        if !solution.unresolved.is_empty()
            || !solution.failed.is_empty()
            || !solution.invalid.is_empty()
        {
            return false;
        }
        let method = self
            .context
            .substitute_callable(&method, &solution.types)
            .bound(1);
        let Some(receipt) =
            self.scopes
                .refine_inferred_binding_receipt(receiver.id.as_str(), key, ty)
        else {
            return false;
        };
        self.write_partial_declaration(
            receiver.id.as_str(),
            key,
            try_inputs::PartialFamily::Set,
            receipt,
        );
        self.partial_sets.get_mut(&key).unwrap().inferred = Some(ty);
        self.bind_call(call, &method);
        true
    }

    fn known_set_item(&self, ty: Type) -> bool {
        if ty == Type::Any {
            return true;
        }
        let ty = self.context.expanded_alias_instance(ty);
        if matches!(ty, Type::Any | Type::AnySpecial | Type::Never) {
            return false;
        }
        let mut variables = Vec::new();
        self.context.collect_variables(ty, &mut variables);
        if !variables.is_empty() {
            return false;
        }
        match self.context.node(ty).as_deref() {
            Some(Node::Container(_, items) | Node::Tuple(items) | Node::Union(items)) => {
                items.iter().all(|&item| self.known_set_item(item))
            }
            Some(Node::VariadicTuple(item) | Node::Unpack(item)) => self.known_set_item(*item),
            _ => true,
        }
    }

    pub(super) fn visit_partial_set_membership_rhs<'a>(&mut self, expression: &'a Expr)
    where
        Self: Visitor<'a>,
    {
        let previous = match expression {
            Expr::Name(name) if name.ctx.is_load() => self
                .partial_set_key(name.id.as_str())
                .or_else(|| self.partial_dict_key(name.id.as_str()))
                .and_then(|key| {
                    let partial = &self.partial_sets[&key];
                    partial
                        .inferred
                        .is_none()
                        .then_some((key, partial.preserve_error))
                }),
            _ => None,
        };
        // Keep all ordinary Name checks and visitor hooks, without committing this read.
        self.visit_expr(expression);
        if let Some((key, preserve_error)) = previous {
            self.partial_sets
                .get_mut(&key)
                .expect("a direct Name visit retains its partial-set record")
                .preserve_error = preserve_error;
        }
    }

    pub(super) fn read_partial_set(&mut self, name: &ExprName) {
        self.read_partial_defaultdict(name);
        self.read_partial_list(name);
        self.read_partial_dict(name);
        if name.ctx.is_load() {
            if let Some(key) = self.partial_set_key(name.id.as_str()) {
                let partial = self.partial_sets.get_mut(&key).unwrap();
                if partial.inferred.is_none() {
                    // Preserve this error, but keep later element inference available.
                    partial.preserve_error = true;
                }
            }
        }
    }

    pub(super) fn reassign_partial_set(&mut self, name: &ExprName) {
        self.reassign_partial_defaultdict(name);
        self.reassign_partial_list(name);
        self.reassign_partial_dict(name);
        if let Some(key) = self.partial_set_key(name.id.as_str()) {
            let partial = self.partial_sets.get_mut(&key).unwrap();
            if partial.last_assignment != name.start() {
                partial.preserve_error |= partial.inferred.is_none();
                partial.active = false;
            }
        }
    }

    pub(super) fn finish_partial_sets(&mut self) {
        let keys: Vec<_> = self
            .partial_sets
            .iter()
            .filter_map(|(&key, partial)| (partial.owner == self.partial_set_scope).then_some(key))
            .collect();
        let mut removed = self.finish_partial_defaultdicts();
        for key in keys {
            let partial = self.partial_sets.remove(&key).unwrap();
            if partial.kind == Container::List
                && self.partial_list_unknown.contains(&key)
                && !partial.preserve_error
            {
                // An unknown lifecycle cannot prove this provisional origin error.
                removed.insert(partial.diagnostic);
                continue;
            }
            if (partial.inferred.is_some() || partial.allow_unresolved_exit)
                && !partial.preserve_error
            {
                removed.insert(partial.diagnostic);
                continue;
            }
            let diagnostic = &mut self.diagnostics[partial.diagnostic];
            if self
                .directives
                .suppresses(diagnostic.location.line, "var-annotated")
            {
                removed.insert(partial.diagnostic);
            } else {
                diagnostic.hint = self
                    .directives
                    .hint(diagnostic.location.line, "var-annotated");
            }
        }
        if removed.is_empty() {
            return;
        }
        // Keep retained order. Captured outer records keep their owner and live index.
        self.diagnostics = std::mem::take(&mut self.diagnostics)
            .into_iter()
            .enumerate()
            .filter_map(|(index, diagnostic)| (!removed.contains(&index)).then_some(diagnostic))
            .collect();
        for partial in self.partial_sets.values_mut() {
            partial.diagnostic -= removed.range(..partial.diagnostic).count();
        }
        for partial in self.partial_defaultdicts.values_mut() {
            partial.diagnostic -= removed.range(..partial.diagnostic).count();
        }
        for range in &mut self.settled_diagnostics {
            let start = range.start;
            let end = range.end;
            range.start -= removed.range(..start).count();
            range.end -= removed.range(..end).count();
        }
        self.remap_loop_diagnostic_ranges(&removed);
    }
}
