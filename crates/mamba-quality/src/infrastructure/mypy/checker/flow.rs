//! Each branch keeps its own refinements and records whether control can continue.
use super::try_inputs::MaterializedInputs;
use super::*;
use quality_syntax::python_ast::{BoolOp, CmpOp, ExprBoolOp, ExprUnaryOp, StmtIf, UnaryOp};
use types::Type;

#[derive(Clone, PartialEq, Eq)]
pub(super) struct Flow {
    pub(super) deleted: std::collections::BTreeSet<String>,
    pub(super) narrowed: Narrowed,
    pub(super) reachable: bool,
}

// Values retain the old map API. A mutable borrow loses proof even if the
// eventual write leaves the same TypeID. No DerefMut caller can keep a stale role.
#[derive(Default, Clone, PartialEq, Eq)]
pub(super) struct Narrowed {
    values: BTreeMap<String, Type>,
    views: BTreeMap<String, super::try_inputs::DeclarationView>,
    unproved: bool,
}

impl std::ops::Deref for Narrowed {
    type Target = BTreeMap<String, Type>;
    fn deref(&self) -> &Self::Target {
        &self.values
    }
}
impl std::ops::DerefMut for Narrowed {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.unproved |= !self.views.is_empty();
        self.views.clear();
        &mut self.values
    }
}
#[derive(Clone, Copy)]
pub(super) enum WithTempKey {
    EnterResult,
    ExitContext,
    ExitArgument,
}

impl WithTempKey {
    pub(super) fn name(self) -> &'static str {
        match self {
            Self::EnterResult => "\0with-enter-result",
            Self::ExitContext => "\0with-exit-context",
            Self::ExitArgument => "\0with-exit-argument",
        }
    }
}

impl Narrowed {
    // These private With values are not user declarations. Update only their
    // value entries and keep declaration proof metadata intact.
    pub(super) fn replace_with_temp(
        &mut self,
        key: WithTempKey,
        value: Option<Type>,
    ) -> Option<Type> {
        match value {
            Some(value) => self.values.insert(key.name().into(), value),
            None => self.values.remove(key.name()),
        }
    }

    pub(super) fn view(&self, name: &str) -> Option<&super::try_inputs::DeclarationView> {
        self.views.get(name)
    }
    pub(super) fn unproved(&self) -> bool {
        self.unproved
    }
    pub(super) fn refuse_declaration_view(&mut self) {
        self.unproved = true;
        self.views.clear();
    }
    pub(super) fn mark_view(&mut self, view: super::try_inputs::DeclarationView) {
        self.views.insert(view.name.clone(), view);
    }
    pub(super) fn declaration_write(
        &mut self,
        name: String,
        ty: Type,
        view: Option<super::try_inputs::DeclarationView>,
    ) {
        if let Some(view) = view {
            self.values.insert(name, ty);
            self.mark_view(view);
        } else {
            self.unproved = true;
            self.views.clear();
            self.values.insert(name, ty);
        }
    }
    fn merge_unproved(&mut self, inputs: &[Flow]) {
        self.unproved |= inputs
            .iter()
            .filter(|input| input.reachable)
            .any(|input| input.narrowed.unproved || !input.narrowed.views.is_empty());
    }
}

impl Default for Flow {
    fn default() -> Self {
        Self {
            deleted: std::collections::BTreeSet::new(),
            narrowed: Narrowed::default(),
            reachable: true,
        }
    }
}

impl Flow {
    pub(super) fn delete_exception_alias(&mut self, name: &str) {
        let member = format!("{name}.");
        let index = format!("{name}[");
        self.narrowed.retain(|key, _| {
            key != name && !key.starts_with(member.as_str()) && !key.starts_with(index.as_str())
        });
        self.deleted.insert(name.into());
    }
}

impl Checker<'_> {
    pub(super) fn environment(&self) -> BTreeMap<String, Type> {
        let mut environment = self.scopes.environment();
        for (name, &narrowed) in self.flow.narrowed.iter() {
            let read = environment
                .get(name)
                .copied()
                .map_or(narrowed, |declared| self.flow_read_type(declared, narrowed));
            environment.insert(name.clone(), read);
        }
        for partial in self.partial_sets.values() {
            if partial.active
                && self.scopes.binding_location(&partial.name) == Some(partial.target.start())
                && (partial.kind != types::Container::Dict
                    || self.scopes.inferred_binding_identity(&partial.name)
                        == partial
                            .binding_owner
                            .map(|owner| (owner, partial.target.start())))
            {
                if let Some(ty) = partial.inferred {
                    environment.insert(partial.name.clone(), ty);
                }
            }
        }
        for name in &self.flow.deleted {
            environment.insert(
                name.clone(),
                self.context.intern(types::Node::Deleted(name.clone())),
            );
        }
        environment
    }

    fn flow_read_type(&self, declared: Type, narrowed: Type) -> Type {
        if declared == narrowed {
            return narrowed;
        }
        // Mypy's narrow_declared_type retains the declaration when the two
        // types differ only in Any origin. Rust identity remains origin-aware.
        if self.context.any_origin_equal(declared, narrowed) {
            return declared;
        }
        let (Some(declared_node), Some(narrowed_node)) =
            (self.context.node(declared), self.context.node(narrowed))
        else {
            return narrowed;
        };
        // narrow_declared_type retains the declaration for equal union types.
        // Union equality ignores order and duplicates. Compare existing member
        // identities only; proper refinements and alias wrappers stay unchanged.
        match (declared_node.as_ref(), narrowed_node.as_ref()) {
            (types::Node::Union(declared_items), types::Node::Union(narrowed_items))
                if declared_items.iter().all(|&declared| {
                    narrowed_items
                        .iter()
                        .any(|&narrowed| self.context.any_origin_equal(declared, narrowed))
                }) && narrowed_items.iter().all(|&narrowed| {
                    declared_items
                        .iter()
                        .any(|&declared| self.context.any_origin_equal(declared, narrowed))
                }) =>
            {
                declared
            }
            _ => narrowed,
        }
    }

    pub(super) fn flow_condition(&mut self, expression: &Expr, positive: bool) {
        if let Some(value) =
            super::super::classes::statements::truth(expression, self.python_target)
        {
            self.flow.reachable &= value == positive;
            return;
        }
        if let Expr::BooleanLiteral(boolean) = expression {
            self.flow.reachable &= boolean.value == positive;
            return;
        }
        if let Expr::BoolOp(boolean) = expression {
            let sequential = (boolean.op == BoolOp::And) == positive;
            if sequential {
                for value in &boolean.values {
                    self.flow_condition(value, positive);
                }
            } else {
                let baseline = self.environment();
                let mut outcomes = Vec::new();
                for value in &boolean.values {
                    let remaining = self.flow.clone();
                    self.flow_condition(value, positive);
                    outcomes.push(self.flow.clone());
                    self.flow = remaining;
                    self.flow_condition(value, !positive);
                }
                self.merge_flow(&outcomes, &baseline);
            }
            return;
        }
        if let Expr::UnaryOp(ExprUnaryOp {
            op: UnaryOp::Not,
            operand,
            ..
        }) = expression
        {
            self.flow_condition(operand, !positive);
            return;
        }
        if let Expr::Call(call) = expression {
            if self.refine_predicate_call(call, positive) {
                return;
            }
        }
        if self.flow_truthiness(expression, positive) {
            return;
        }
        let predicate = match expression {
            Expr::Compare(compare) if compare.operands.len() == 2 => {
                let value = match (&compare.operands[0], &compare.operands[1]) {
                    (Expr::Name(name), value) | (value, Expr::Name(name)) => self
                        .context
                        .literal_expression(value)
                        .map(|ty| (name.id.as_str(), ty)),
                    _ => None,
                };
                if let (Some((name, target)), Some(operator)) = (value, compare.ops.first()) {
                    if target != Type::None
                        && matches!(
                            operator,
                            CmpOp::Eq | CmpOp::NotEq | CmpOp::Is | CmpOp::IsNot
                        )
                    {
                        if let Some(&ty) = self.environment().get(name) {
                            let positive = if matches!(operator, CmpOp::NotEq | CmpOp::IsNot) {
                                !positive
                            } else {
                                positive
                            };
                            let narrowed = self.context.literal_condition(
                                ty,
                                target,
                                positive,
                                matches!(operator, CmpOp::Is | CmpOp::IsNot),
                            );
                            self.flow.narrowed.insert(name.into(), narrowed);
                            self.flow.reachable &= narrowed != Type::Never;
                        }
                        return;
                    }
                }
                let value = match (&compare.operands[0], &compare.operands[1]) {
                    (value, Expr::NoneLiteral(_)) | (Expr::NoneLiteral(_), value)
                        if matches!(value, Expr::Name(_) | Expr::Attribute(_)) =>
                    {
                        Some(value)
                    }
                    _ => None,
                };
                match (value, compare.ops.first()) {
                    (Some(value), Some(CmpOp::Is)) => Some((value, Type::None, positive)),
                    (Some(value), Some(CmpOp::IsNot)) => Some((value, Type::None, !positive)),
                    _ => None,
                }
            }
            _ => None,
        };
        if let Some((value, target, positive)) = predicate {
            if let (Some(key), Some(ty)) = (scopes::Scopes::attribute_key(value), self.infer(value))
            {
                let narrowed = self.context.narrow(ty, target, positive);
                self.flow.narrowed.insert(key, narrowed);
                if narrowed == Type::Never {
                    self.flow.reachable = false;
                }
            }
        }
    }

    pub(super) fn visit_boolean<'a>(&mut self, boolean: &'a ExprBoolOp)
    where
        Self: Visitor<'a>,
    {
        let previous = self.flow.clone();
        for (index, value) in boolean.values.iter().enumerate() {
            if !self.flow.reachable {
                break;
            }
            self.visit_expr(value);
            if index + 1 != boolean.values.len() {
                self.flow_condition(value, boolean.op == BoolOp::And);
            }
        }
        self.flow = previous;
    }

    pub(super) fn check_if<'a>(&mut self, conditional: &'a StmtIf)
    where
        Self: Visitor<'a>,
    {
        let baseline = self.environment();
        let mut outcomes = Vec::new();
        self.visit_expr(&conditional.test);
        let remaining = self.flow.clone();
        self.flow_condition(&conditional.test, true);
        self.visit_block(&conditional.body);
        outcomes.push(self.flow.clone());
        self.flow = remaining;
        self.flow_condition(&conditional.test, false);
        let mut has_else = false;
        for clause in &conditional.elif_else_clauses {
            if let Some(test) = &clause.test {
                self.visit_expr(test);
                let remaining = self.flow.clone();
                self.flow_condition(test, true);
                self.visit_block(&clause.body);
                outcomes.push(self.flow.clone());
                self.flow = remaining;
                self.flow_condition(test, false);
            } else {
                has_else = true;
                self.visit_block(&clause.body);
                outcomes.push(self.flow.clone());
            }
        }
        if !has_else {
            outcomes.push(self.flow.clone());
        }
        self.merge_flow(&outcomes, &baseline);
    }

    pub(super) fn merge_flow(&mut self, outcomes: &[Flow], baseline: &BTreeMap<String, Type>) {
        let reachable: Vec<_> = outcomes.iter().filter(|flow| flow.reachable).collect();
        let mut flow = Flow {
            deleted: reachable
                .first()
                .map(|first| {
                    first
                        .deleted
                        .iter()
                        .filter(|name| reachable.iter().all(|flow| flow.deleted.contains(*name)))
                        .cloned()
                        .collect()
                })
                .unwrap_or_default(),
            reachable: !reachable.is_empty(),
            narrowed: Narrowed::default(),
        };
        let names: std::collections::BTreeSet<_> = outcomes
            .iter()
            .flat_map(|flow| flow.narrowed.keys())
            .collect();
        let mut lexical_declarations = None;
        for name in names {
            let types: Option<Vec<_>> = reachable
                .iter()
                .map(|flow| {
                    flow.narrowed
                        .get(name)
                        .or_else(|| baseline.get(name))
                        .copied()
                })
                .collect();
            if let Some(types) = types.filter(|types| !types.is_empty()) {
                let mixed_any = types.iter().any(|ty| ty.is_any())
                    && types.iter().any(|&ty| self.known_non_any_branch_type(ty));
                // The baseline can contain a refinement. Only the lexical
                // declaration view may trigger Any's branch-merge rule.
                let merged = if mixed_any
                    && !name.contains(['.', '['])
                    && self
                        .scopes
                        .resolved_binding(name)
                        .is_some_and(|binding| binding.module.is_none())
                    && lexical_declarations
                        .get_or_insert_with(|| self.scopes.environment())
                        .get(name)
                        .is_some_and(|ty| ty.is_any())
                {
                    Type::Any
                } else {
                    self.context
                        .truth_merge(types, self.declared_flow_type(name, baseline))
                };
                let merged = self.merge_inferred_declaration_any(
                    name,
                    merged,
                    mixed_any,
                    &mut lexical_declarations,
                );
                flow.narrowed.insert(name.clone(), merged);
            }
        }
        flow.narrowed.merge_unproved(outcomes);
        self.flow = flow;
    }

    pub(super) fn merge_commit_inputs(&mut self, inputs: MaterializedInputs) {
        let MaterializedInputs {
            baseline,
            options,
            deleted,
            count,
            role_lost,
        } = inputs;
        let mut flow = flow::Flow {
            deleted,
            narrowed: Narrowed::default(),
            reachable: count != 0,
        };
        let mut lexical_declarations = None;
        for (name, types) in options {
            if let Some(types) = types.filter(|types| !types.is_empty()) {
                let mixed_any = types.iter().any(|ty| ty.is_any())
                    && types.iter().any(|&ty| self.known_non_any_branch_type(ty));
                // Apply the same current lexical-declaration rule as merge_flow.
                // The fixed entry baseline is used only for missing-key options
                // and declared_flow_type, never as a declaration substitute.
                let merged = if mixed_any
                    && !name.contains(['.', '['])
                    && self
                        .scopes
                        .resolved_binding(&name)
                        .is_some_and(|binding| binding.module.is_none())
                    && lexical_declarations
                        .get_or_insert_with(|| self.scopes.environment())
                        .get(&name)
                        .is_some_and(|ty| ty.is_any())
                {
                    Type::Any
                } else {
                    self.context
                        .truth_merge(types, self.declared_flow_type(&name, &baseline))
                };
                let merged = self.merge_inferred_declaration_any(
                    &name,
                    merged,
                    mixed_any,
                    &mut lexical_declarations,
                );
                flow.narrowed.insert(name, merged);
            }
        }
        // Materialization preserves values, but does not prove a merged value's
        // declaration role. Never silently reuse it as fresh Lexical evidence.
        flow.narrowed.unproved |= role_lost;
        self.flow = flow;
    }

    fn merge_inferred_declaration_any(
        &self,
        name: &str,
        merged: Type,
        distinct_any_inputs: bool,
        lexical_declarations: &mut Option<BTreeMap<String, Type>>,
    ) -> Type {
        // A direct Any and a known material input are distinct even when Any
        // origins are ignored. A merged Union alone cannot prove this branch.
        if !distinct_any_inputs {
            return merged;
        }
        let node = self.context.node(merged);
        let Some(types::Node::Union(items)) = node.as_deref() else {
            return merged;
        };
        if !items.iter().any(|ty| ty.is_any())
            || items
                .iter()
                .any(|&ty| !ty.is_any() && !self.known_non_any_branch_type(ty))
            || name.contains(['.', '['])
            || self.scopes.inferred_binding_identity(name).is_none()
            || !self
                .scopes
                .resolved_binding(name)
                .is_some_and(|binding| binding.module.is_none())
        {
            return merged;
        }
        let Some(&declared) = lexical_declarations
            .get_or_insert_with(|| self.scopes.environment())
            .get(name)
        else {
            return merged;
        };
        if declared.is_any() {
            return merged;
        }
        // Mypy binder.update_from_options removes extra top-level Any only
        // when the result equals the actual declaration under old semantics.
        // The entry baseline may be narrowed and is not declaration evidence.
        let simplified = self
            .context
            .union(items.iter().copied().filter(|ty| !ty.is_any()).collect());
        if simplified == declared {
            declared
        } else {
            merged
        }
    }

    fn known_non_any_branch_type(&self, ty: Type) -> bool {
        matches!(
            ty,
            Type::Int
                | Type::Float
                | Type::Complex
                | Type::Str
                | Type::Bytes
                | Type::Bool
                | Type::None
                | Type::Object
        ) || self.context.node(ty).is_some_and(|node| {
            matches!(
                node.as_ref(),
                types::Node::Container(..)
                    | types::Node::Tuple(_)
                    | types::Node::VariadicTuple(_)
                    | types::Node::Instance(_)
                    | types::Node::Class(_)
                    | types::Node::AppliedClass { .. }
                    | types::Node::Callable(_)
                    | types::Node::Literal(_)
                    | types::Node::LiteralValue(_)
            )
        })
    }

    pub(crate) fn visit_block<'a>(&mut self, body: &'a [Stmt])
    where
        Self: Visitor<'a>,
    {
        self.check_overload_groups(body);
        for statement in body {
            if !self.flow.reachable {
                break;
            }
            self.visit_stmt(statement);
        }
    }

    pub(super) fn flow_assignment(&mut self, target: &Expr, actual: Option<Type>) {
        self.check_try_member_inputs(Some(target));
        if let Some(key) = scopes::Scopes::attribute_key(target) {
            let member_prefix = format!("{key}.");
            let index_prefix = format!("{key}[");
            self.flow.narrowed.retain(|name, _| {
                !name.starts_with(member_prefix.as_str())
                    && !name.starts_with(index_prefix.as_str())
            });
        }
        if target.is_attribute_expr() {
            if let Some(key) = scopes::Scopes::attribute_key(target) {
                self.flow.narrowed.remove(&key);
                if let Some((expected, actual)) = self.assigned_object_type(target).zip(actual) {
                    if self.context.node(expected).is_some_and(|node| {
                        matches!(node.as_ref(), types::Node::Tuple(items) if self.context.tuple_pack_position(items).is_some())
                    }) && self.context.accepts(expected, actual) {
                        self.flow.narrowed.insert(key, self.context.tuple_assignment_type(expected, actual));
                    } else if let Some(narrowed) = self.ordinary_member_assignment_type(target, expected, actual) {
                        self.flow.narrowed.insert(key, narrowed);
                    } else {
                        self.project_try_member_assignment(target, expected, actual);
                    }
                }
            }
        }
        if let Expr::Name(name) = target {
            self.reassign_partial_set(name);
            self.flow.deleted.remove(name.id.as_str());
            if let Some(actual) = actual {
                if let Some(expected) = self.scopes.assigned_type(target) {
                    // Mypy's binder invalidates dependencies before this check,
                    // but a rejected assignment keeps the Name's own restriction.
                    if !self.context.accepts(expected, actual) {
                        return;
                    }
                    self.flow.narrowed.insert(
                        name.id.to_string(),
                        if expected.is_any()
                            || self.context.node(expected).is_some_and(|node| {
                                matches!(node.as_ref(), types::Node::Callable(_))
                            })
                        {
                            expected
                        } else {
                            actual
                        },
                    );
                    return;
                }
            }
            self.flow.narrowed.remove(name.id.as_str());
        }
    }

    fn ordinary_member_assignment_type(
        &self,
        target: &Expr,
        expected: Type,
        actual: Type,
    ) -> Option<Type> {
        // Only this closed declaration shape has no descriptor-valued arm.
        // A bare Any declaration keeps its existing assignment/read policy.
        let declared = self.context.node(expected)?;
        let types::Node::Union(items) = declared.as_ref() else {
            return None;
        };
        if items.len() != 2
            || !items.contains(&Type::None)
            || !items.iter().any(|ty| ty.is_any())
            || !self.known_instance_actual(actual)
        {
            return None;
        }
        let Expr::Attribute(attribute) = target else {
            return None;
        };
        let Expr::Name(root) = attribute.value.as_ref() else {
            return None;
        };
        let receiver = *self.environment().get(root.id.as_str())?;
        // Match the existing Name inference authority without inferring it again.
        if self
            .context
            .observed_expression_type(&attribute.value)
            .is_some_and(|observed| observed != Some(receiver))
        {
            return None;
        }
        let id = self.plain_member_receiver(receiver)?;
        let class = self.context.class(id);
        let name = attribute.attr.as_str();
        if class.decorated_fields.contains(name)
            || class.cached_properties.contains(name)
            || class.methods.contains_key(name)
        {
            return None;
        }
        let field = class.fields.get(name)?;
        if field.ty != Some(expected)
            || field.class_access
            || field.class_var
            || field.readonly
            || field.final_attribute
            || field.abstract_field
            || field.needs_annotation
        {
            return None;
        }
        if self.context.class_member_owner(receiver, name) != Some(id)
            || self.context.raw_class_field(receiver, name)?.ty != Some(expected)
            || self.context.class_field(receiver, name)?.ty != Some(expected)
            || self
                .context
                .descriptor_member(receiver, name)
                .is_some_and(|ty| ty != expected)
        {
            return None;
        }
        // The stored, read and already-computed write types agree. No declaration
        // is changed, and only this Name-root member key receives the actual type.
        self.context.accepts(expected, actual).then_some(actual)
    }

    pub(super) fn try_int_member_owner(&self, target: &Expr, receiver: Type) -> Option<u32> {
        let Expr::Attribute(attribute) = target else {
            return None;
        };
        self.try_int_member_field_owner(attribute.attr.as_str(), receiver)
    }

    pub(super) fn try_int_member_field_owner(&self, name: &str, receiver: Type) -> Option<u32> {
        if self.flow.narrowed.unproved || !self.flow.narrowed.views.is_empty() {
            return None;
        }
        let id = self.plain_member_receiver(receiver)?;
        let class = self.context.class(id);
        if class.decorated_fields.contains(name)
            || class.cached_properties.contains(name)
            || class.methods.contains_key(name)
        {
            return None;
        }
        let field = class.fields.get(name)?;
        if field.ty != Some(Type::Int)
            || field.class_access
            || field.class_var
            || field.readonly
            || field.final_attribute
            || field.abstract_field
            || field.needs_annotation
            || self.context.class_member_owner(receiver, name) != Some(id)
            || self.context.raw_class_field(receiver, name)?.ty != Some(Type::Int)
            || self.context.class_field(receiver, name)?.ty != Some(Type::Int)
            || self.context.class_write_type(receiver, name) != Some(Type::Int)
            || self.context.descriptor_member(receiver, name).is_some()
        {
            return None;
        }
        Some(id)
    }

    fn plain_member_receiver(&self, receiver: Type) -> Option<u32> {
        let receiver_node = self.context.node(receiver)?;
        let types::Node::Instance(id) = receiver_node.as_ref() else {
            return None;
        };
        let class = self.context.class(*id);
        if class.namespace != self.scopes.modules.as_ref()?.namespace()
            || self.context.module_is_stub(&class.namespace)
            || !class.generic.is_empty()
            || !class.scalar_bases.is_empty()
            || !class.any_base_origins.is_empty()
            || class.plugin
            || class.protocol
            || class.typed_dict.is_some()
            || class.pydantic.is_some()
            || class.dataclass.is_some()
            || !class.erased_members.is_empty()
            || self.context.recorded_metaclass(*id).is_some()
            || [
                "__getattr__",
                "__getattribute__",
                "__setattr__",
                "__delattr__",
            ]
            .iter()
            .any(|name| class.methods.contains_key(*name) || class.fields.contains_key(*name))
        {
            return None;
        }
        if class.bases.len() != 1
            || self.context.class(class.bases[0]).scalar_bases.as_slice() != [Type::Object]
        {
            return None;
        }
        let object = self.context.builtin_class(Type::Object);
        if class.bases.as_slice() != [object]
            || class.base_types.as_slice() != [self.context.class(object).instance]
        {
            return None;
        }
        let ready = {
            let registry = self.context.classes.borrow();
            [*id, object]
                .iter()
                .all(|id| registry.populated_class_bases.get(id) == Some(&true))
        };
        if !ready || self.context.class_mro(*id).ok()?.as_slice() != [*id, object] {
            return None;
        }
        Some(*id)
    }

    fn known_instance_actual(&self, actual: Type) -> bool {
        let Some(node) = self.context.node(actual) else {
            return false;
        };
        let (id, arity) = match node.as_ref() {
            types::Node::Instance(id) => (*id, 0),
            types::Node::AppliedClass {
                id,
                arguments,
                value: false,
            } if arguments.iter().all(|ty| {
                matches!(
                    ty,
                    Type::Int
                        | Type::Float
                        | Type::Complex
                        | Type::Str
                        | Type::Bytes
                        | Type::Bool
                        | Type::Object
                        | Type::None
                )
            }) =>
            {
                (*id, arguments.len())
            }
            _ => return false,
        };
        let class = self.context.class(id);
        class.generic.len() == arity
            && !class.plugin
            && !class.protocol
            && class.typed_dict.is_none()
            && class.any_base_origins.is_empty()
    }
}
