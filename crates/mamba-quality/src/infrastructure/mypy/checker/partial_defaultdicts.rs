//! A proved ambiguous factory retains its nominal partial dictionary owner.
use super::*;
use quality_syntax::python_ast::{ExprCall, ExprName, ExprSubscript, StmtAssign};
use ruff_text_size::TextSize;
use types::{Container, Node, Type};

#[derive(Clone, PartialEq, Eq)]
pub(super) struct Seed {
    pub class_id: u32,
    pub family: Container,
    pub initial: Type,
    pub factory: types::constraints::FactoryAmbiguity,
}

#[derive(Clone)]
pub(super) struct PartialDefaultdict {
    pub name: String,
    pub target: Expr,
    pub seed: Seed,
    pub inferred: Option<Type>,
    pub active: bool,
    pub last_assignment: TextSize,
    pub preserve_error: bool,
    pub diagnostic: usize,
    pub owner: usize,
    pub binding_owner: usize,
    pub allow_unresolved_exit: bool,
}

// Storage indices and cloned AST addresses do not determine convergence.
#[derive(PartialEq, Eq)]
pub(super) struct SemanticState {
    origin: TextSize,
    target: ruff_text_size::TextRange,
    name: String,
    seed: Seed,
    inferred: Option<Type>,
    active: bool,
    last_assignment: TextSize,
    preserve_error: bool,
    owner: usize,
    binding_owner: usize,
    allow_unresolved_exit: bool,
}

impl Checker<'_> {
    pub(super) fn partial_defaultdict_state(&self) -> Vec<SemanticState> {
        self.partial_defaultdicts
            .iter()
            .map(|(&origin, value)| SemanticState {
                origin,
                target: value.target.range(),
                name: value.name.clone(),
                seed: value.seed.clone(),
                inferred: value.inferred,
                active: value.active,
                last_assignment: value.last_assignment,
                preserve_error: value.preserve_error,
                owner: value.owner,
                binding_owner: value.binding_owner,
                allow_unresolved_exit: value.allow_unresolved_exit,
            })
            .collect()
    }

    fn partial_defaultdict_key(&self, name: &str) -> Option<TextSize> {
        let (owner, origin) = self.scopes.inferred_binding_identity(name)?;
        self.partial_defaultdicts
            .get(&origin)
            .filter(|partial| {
                partial.active && partial.name == name && partial.binding_owner == owner
            })
            .map(|_| origin)
    }

    pub(super) fn partial_defaultdict_assignment_seed(
        &self,
        assignment: &StmtAssign,
    ) -> Option<Seed> {
        let [target @ Expr::Name(name)] = assignment.targets.as_slice() else {
            return None;
        };
        if !self.flow.reachable || name.id.as_str() == "_" {
            return None;
        }
        let replay = self
            .scopes
            .inferred_binding_identity(name.id.as_str())
            .and_then(|(owner, origin)| {
                self.partial_defaultdicts
                    .get(&origin)
                    .filter(|partial| {
                        partial.binding_owner == owner && partial.name == name.id.as_str()
                    })
                    .map(|_| origin)
            });
        if replay.is_none()
            && (self.scopes.assigned_type(target).is_some()
                || self.scopes.first_inferred_local_assignment(target)?.1 != name.start())
        {
            return None;
        }
        let (class_id, family, initial, factory) = self
            .scopes
            .partial_defaultdict_seed(&assignment.value, &self.environment())?;
        let mut seed = Seed {
            class_id,
            family,
            initial,
            factory,
        };
        if let Some(key) = replay {
            let partial = &self.partial_defaultdicts[&key];
            if key != name.start()
                || partial.seed.factory != seed.factory
                || partial.seed.class_id != seed.class_id
                || partial.seed.family != seed.family
                || partial.owner != self.partial_set_scope
                || !self.scopes.replayed_loop_definition(name.id.as_str(), key)
                || !self.context.source_declaration_site(name)
            {
                return None;
            }
            // A backedge revisits the same initializer, not a new assignment.
            seed.initial = partial.inferred.unwrap_or(seed.initial);
        }
        Some(seed)
    }

    pub(super) fn remember_partial_defaultdict(
        &mut self,
        assignment: &StmtAssign,
        seed: &Seed,
    ) -> bool {
        let [target @ Expr::Name(name)] = assignment.targets.as_slice() else {
            return false;
        };
        if let Some(partial) = self.partial_defaultdicts.get(&name.start()) {
            return partial.name == name.id.as_str()
                && partial.owner == self.partial_set_scope
                && partial.seed.factory == seed.factory
                && partial.seed.class_id == seed.class_id
                && self.scopes.inferred_binding_identity(name.id.as_str())
                    == Some((partial.binding_owner, name.start()));
        }
        let Some((binding_owner, origin)) = self.scopes.inferred_binding_identity(name.id.as_str())
        else {
            return false;
        };
        if origin != name.start() || self.scopes.assigned_type(target).is_some() {
            return false;
        }
        let diagnostic = self.diagnostics.len();
        // Original leave_partial_types uses its saved lvalue, with no defaultdict hint.
        self.diagnostics
            .push(crate::infrastructure::diagnostics::at(
                self.source,
                QualityTool::Mypy,
                "var-annotated",
                format!("Need type annotation for \"{}\"", name.id),
                target.range(),
            ));
        self.partial_defaultdicts.insert(
            origin,
            PartialDefaultdict {
                name: name.id.to_string(),
                target: target.clone(),
                seed: seed.clone(),
                inferred: None,
                active: true,
                last_assignment: origin,
                preserve_error: false,
                diagnostic,
                owner: self.partial_set_scope,
                binding_owner,
                allow_unresolved_exit: self.partial_dict_dynamic,
            },
        );
        true
    }

    pub(super) fn prepare_partial_defaultdict_assignment(
        &mut self,
        assignment: &StmtAssign,
        actual: Option<Type>,
    ) {
        let [Expr::Name(name)] = assignment.targets.as_slice() else {
            return;
        };
        let Some(key) = self.partial_defaultdict_key(name.id.as_str()) else {
            return;
        };
        if key == name.start()
            && self.scopes.replayed_loop_definition(name.id.as_str(), key)
            && self.context.source_declaration_site(name)
        {
            return;
        }
        let solved = actual.filter(|&ty| self.valid_complete_defaultdict(key, ty));
        if let Some(ty) = solved {
            if self.resolve_partial_defaultdict(key, ty) {
                self.partial_defaultdicts
                    .get_mut(&key)
                    .unwrap()
                    .last_assignment = name.start();
                return;
            }
        }
        let partial = self.partial_defaultdicts.get_mut(&key).unwrap();
        partial.preserve_error |= partial.inferred.is_none();
        partial.active = false;
    }

    pub(super) fn read_partial_defaultdict(&mut self, name: &ExprName) {
        if (name.ctx.is_load() || name.ctx.is_del())
            && !self.partial_dict_lvalues.contains(&name.start())
            && !self.flow.deleted.contains(name.id.as_str())
        {
            if let Some(key) = self.partial_defaultdict_key(name.id.as_str()) {
                let partial = self.partial_defaultdicts.get_mut(&key).unwrap();
                let unresolved = partial.inferred.is_none();
                partial.preserve_error |= unresolved;
                if unresolved {
                    self.partial_defaultdict_gaps.insert(format!(
                        "Partial defaultdict unresolved read recovery is unsupported at byte {}",
                        name.start().to_u32(),
                    ));
                }
            }
        }
    }

    pub(super) fn reassign_partial_defaultdict(&mut self, name: &ExprName) {
        if let Some(key) = self.partial_defaultdict_key(name.id.as_str()) {
            let partial = self.partial_defaultdicts.get_mut(&key).unwrap();
            if partial.last_assignment != name.start() {
                partial.preserve_error |= partial.inferred.is_none();
                partial.active = false;
            }
        }
    }

    fn partial_defaultdict_mutation<'a>(
        &self,
        expression: &'a Expr,
    ) -> Option<(
        &'a ExprCall,
        &'a quality_syntax::python_ast::ExprAttribute,
        &'a ExprSubscript,
        &'a ExprName,
        TextSize,
    )> {
        let Expr::Call(call) = expression else {
            return None;
        };
        let Expr::Attribute(attribute) = call.func.as_ref() else {
            return None;
        };
        let Expr::Subscript(index) = attribute.value.as_ref() else {
            return None;
        };
        let Expr::Name(name) = index.value.as_ref() else {
            return None;
        };
        let key = self.partial_defaultdict_key(name.id.as_str())?;
        let partial = &self.partial_defaultdicts[&key];
        if partial.inferred.is_some()
            || !self.flow.reachable
            || !self.context.source_declaration_site(expression)
            || !self.context.source_declaration_site(call.func.as_ref())
            || !self
                .context
                .source_declaration_site(attribute.value.as_ref())
            || !(index.end() < attribute.end() && attribute.end() < call.end())
            || self.flow.deleted.contains(name.id.as_str())
            || !matches!(
                (partial.seed.family, attribute.attr.as_str()),
                (Container::List, "append") | (Container::Set, "add")
            )
            || !matches!(call.arguments.args.as_ref(), [argument] if !argument.is_starred_expr())
            || !call.arguments.keywords.is_empty()
        {
            return None;
        }
        Some((call, attribute, index, name, key))
    }

    pub(super) fn visit_partial_defaultdict_call<'a>(&mut self, expression: &'a Expr) -> bool
    where
        Self: Visitor<'a>,
    {
        if self
            .partial_defaultdict_deferred
            .contains(&(expression.start(), expression.end()))
        {
            return false;
        }
        let Some((call, attribute, index, name, key)) =
            self.partial_defaultdict_mutation(expression)
        else {
            return false;
        };
        if !self.partial_defaultdict_read_only_operand(&index.slice)
            || !self.partial_defaultdict_read_only_operand(&call.arguments.args[0])
        {
            self.partial_defaultdict_gaps.insert(format!(
                "Partial defaultdict mutation key/item visit order is unsupported at byte {}",
                call.start().to_u32(),
            ));
            return false;
        }
        // Only these three nodes defer their normal checks. Normal walk_expr
        // still reaches every visit_expr wrapper, observer, and child once.
        // A recursive entry sees the marker and cannot claim this call again.
        let ranges = [expression.range(), attribute.range(), index.range()]
            .map(|range| (range.start(), range.end()));
        if ranges
            .iter()
            .any(|range| self.partial_defaultdict_deferred.contains(range))
        {
            return false;
        }
        self.partial_defaultdict_deferred.extend(ranges);
        let inserted = self.partial_dict_lvalues.insert(name.start());
        self.visit_expression(expression);
        if inserted {
            self.partial_dict_lvalues.remove(&name.start());
        }
        for range in ranges {
            self.partial_defaultdict_deferred.remove(&range);
        }
        if self.flow.reachable
            && self.partial_defaultdict_key(name.id.as_str()) == Some(key)
            && self.partial_defaultdicts[&key].inferred.is_none()
        {
            let key_type = self.infer(&index.slice);
            let item_type = self.infer(&call.arguments.args[0]);
            if let Some((key_type, item_type)) =
                key_type.zip(item_type).filter(|&(key_type, item_type)| {
                    self.valid_defaultdict_item(key_type, true)
                        && self.valid_defaultdict_item(item_type, true)
                })
            {
                let seed = &self.partial_defaultdicts[&key].seed;
                let value = self
                    .context
                    .intern(Node::Container(seed.family, vec![item_type]));
                let ty = self
                    .context
                    .applied_class(seed.class_id, vec![key_type, value], false);
                self.resolve_partial_defaultdict(key, ty);
            }
        }
        // A failed mutation falls through to a real read of the pending base.
        // A later successful store cannot erase that earlier error obligation.
        if let Some(partial) = self.partial_defaultdicts.get_mut(&key) {
            partial.preserve_error |= partial.inferred.is_none();
        }
        // These were deferred only for the qualified pending mutation. Each
        // child has now been visited once, and each normal checker runs once.
        self.check_nominal_subscript(index);
        self.check_class_index(index);
        self.check_typed_dict_index(index);
        self.check_dictionary_index(index);
        self.check_expression_attribute(call.func.as_ref(), attribute);
        self.check_model_call(expression);
        self.check_function_call(expression);
        true
    }

    // This classifies checker effects, not Python runtime purity. Ordinary
    // nongeneric function calls do not execute their bodies in the checker.
    // Unresolved partial reads need the original error-Any recovery, which
    // this visitor does not implement. Keep those operands on the old route.
    // Unknown/special operations also keep the old route.
    fn partial_defaultdict_read_only_operand(&self, expression: &Expr) -> bool {
        match expression {
            Expr::Name(name) => !self.partial_defaultdict_operand_reads_unresolved(name),
            Expr::NumberLiteral(_)
            | Expr::StringLiteral(_)
            | Expr::BytesLiteral(_)
            | Expr::BooleanLiteral(_)
            | Expr::NoneLiteral(_) => true,
            Expr::Tuple(tuple) => tuple
                .elts
                .iter()
                .all(|value| self.partial_defaultdict_read_only_operand(value)),
            Expr::BinOp(binary) => {
                matches!(
                    binary.op,
                    quality_syntax::python_ast::Operator::Add
                        | quality_syntax::python_ast::Operator::FloorDiv
                ) && self.partial_defaultdict_read_only_operand(&binary.left)
                    && self.partial_defaultdict_read_only_operand(&binary.right)
                    && matches!(self.infer(&binary.left), Some(Type::Int | Type::Bool))
                    && matches!(self.infer(&binary.right), Some(Type::Int | Type::Bool))
            }
            Expr::Call(call) => {
                // This syntax has an early observer/return path even when a
                // same-named ordinary source function shadows the builtin.
                if matches!(call.func.as_ref(), Expr::Name(name) if name.id.as_str() == "reveal_type")
                {
                    return false;
                }
                if !matches!(call.func.as_ref(), Expr::Name(_) | Expr::Attribute(_))
                    || matches!(call.func.as_ref(), Expr::Attribute(attribute)
                        if self.scopes.module_expression(&attribute.value).is_none())
                    || call.arguments.args.iter().any(Expr::is_starred_expr)
                    || call
                        .arguments
                        .keywords
                        .iter()
                        .any(|keyword| keyword.arg.is_none())
                    || !call
                        .arguments
                        .args
                        .iter()
                        .all(|value| self.partial_defaultdict_read_only_operand(value))
                    || !call
                        .arguments
                        .keywords
                        .iter()
                        .all(|keyword| self.partial_defaultdict_read_only_operand(&keyword.value))
                {
                    return false;
                }
                let Some(actual) = self.infer(&call.func) else {
                    return false;
                };
                let Some(node) = self.context.node(actual) else {
                    return false;
                };
                let Node::Callable(signature) = node.as_ref() else {
                    return false;
                };
                !signature.constructor
                    && !signature.ignored
                    && signature.native_symbol.is_none()
                    && signature.owner.is_none()
                    && signature.bound_receiver.is_none()
                    && signature.narrowing.is_none()
                    && signature.generic.is_empty()
                    && signature.overloads.is_empty()
                    && signature
                        .returns
                        .is_some_and(|ty| self.valid_defaultdict_item(ty, true))
            }
            _ => false,
        }
    }

    fn partial_defaultdict_operand_reads_unresolved(&self, name: &ExprName) -> bool {
        if !name.ctx.is_load() || self.flow.deleted.contains(name.id.as_str()) {
            return false;
        }
        let Some((owner, origin)) = self.scopes.inferred_binding_identity(name.id.as_str()) else {
            return false;
        };
        let defaultdict = self
            .partial_defaultdicts
            .get(&origin)
            .is_some_and(|partial| {
                partial.active
                    && partial.inferred.is_none()
                    && partial.name == name.id.as_str()
                    && partial.binding_owner == owner
            });
        // The existing container map has no read-specific recovery either.
        // Its current raw type can already contain Any, but an unresolved
        // record alone cannot prove the original read/order behavior.
        let container = self.partial_sets.get(&origin).is_some_and(|partial| {
            partial.active
                && partial.inferred.is_none()
                && partial.name == name.id.as_str()
                && partial.binding_owner == Some(owner)
                && matches!(
                    partial.kind,
                    Container::List | Container::Set | Container::Dict
                )
        });
        defaultdict || container
    }

    fn resolve_partial_defaultdict(&mut self, key: TextSize, ty: Type) -> bool {
        let partial = &self.partial_defaultdicts[&key];
        if self.scopes.inferred_binding_identity(&partial.name)
            != Some((partial.binding_owner, key))
        {
            return false;
        }
        let name = partial.name.clone();
        let Some(receipt) = self.scopes.refine_inferred_binding_receipt(&name, key, ty) else {
            return false;
        };
        let partial = self.partial_defaultdicts.get_mut(&key).unwrap();
        partial.inferred = Some(ty);
        partial.active = false;
        self.write_partial_declaration(&name, key, try_inputs::PartialFamily::Defaultdict, receipt);
        true
    }

    fn valid_complete_defaultdict(&self, key: TextSize, ty: Type) -> bool {
        let Some(node) = self.context.node(ty) else {
            return false;
        };
        let Node::AppliedClass {
            id,
            arguments,
            value: false,
        } = node.as_ref()
        else {
            return false;
        };
        let seed = &self.partial_defaultdicts[&key].seed;
        if *id != seed.class_id
            || arguments.len() != 2
            || !self.valid_defaultdict_item(arguments[0], true)
        {
            return false;
        }
        matches!(self.context.node(arguments[1]).as_deref(), Some(Node::Container(family, items))
            if *family == seed.family && items.len() == 1 && self.valid_defaultdict_item(items[0], true))
    }

    fn valid_defaultdict_item(&self, ty: Type, top: bool) -> bool {
        if ty == Type::Never || (top && ty == Type::None) {
            return false;
        }
        match self.context.node(ty).as_deref() {
            Some(
                Node::Container(_, items)
                | Node::Tuple(items)
                | Node::Union(items)
                | Node::AppliedClass {
                    arguments: items,
                    value: false,
                    ..
                },
            ) => items
                .iter()
                .all(|&item| self.valid_defaultdict_item(item, false)),
            Some(Node::VariadicTuple(item) | Node::Unpack(item)) => {
                self.valid_defaultdict_item(*item, false)
            }
            Some(
                Node::Variable(_)
                | Node::ParamSpecArgs(_)
                | Node::ParamSpecKwargs(_)
                | Node::Parameters(_, _)
                | Node::Alias(_, _)
                | Node::AliasInstance(_, _)
                | Node::InferredAlias { .. }
                | Node::LiteralForm
                | Node::TypingForm(_)
                | Node::Deleted(_),
            ) => false,
            Some(_) => {
                let mut variables = Vec::new();
                self.context.collect_variables(ty, &mut variables);
                variables.is_empty()
            }
            None => matches!(
                ty,
                Type::Any
                    | Type::AnySpecial
                    | Type::Object
                    | Type::None
                    | Type::Bool
                    | Type::Int
                    | Type::Float
                    | Type::Complex
                    | Type::Str
                    | Type::Bytes
            ),
        }
    }

    pub(super) fn finish_partial_defaultdicts(&mut self) -> BTreeSet<usize> {
        let keys: Vec<_> = self
            .partial_defaultdicts
            .iter()
            .filter_map(|(&key, value)| (value.owner == self.partial_set_scope).then_some(key))
            .collect();
        let mut removed = BTreeSet::new();
        for key in keys {
            let value = self.partial_defaultdicts.remove(&key).unwrap();
            if (value.inferred.is_some() || value.allow_unresolved_exit) && !value.preserve_error {
                removed.insert(value.diagnostic);
            } else {
                let diagnostic = &mut self.diagnostics[value.diagnostic];
                if self
                    .directives
                    .suppresses(diagnostic.location.line, "var-annotated")
                {
                    removed.insert(value.diagnostic);
                } else {
                    diagnostic.hint = self
                        .directives
                        .hint(diagnostic.location.line, "var-annotated");
                }
            }
        }
        removed
    }
}
