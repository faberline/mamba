//! A checked plain Int store publishes one real member binder input.
use super::*;
use quality_syntax::python_ast::Number;

#[derive(Clone)]
pub(super) struct MemberWrite {
    // The immutable statement owns this target and direct literal RHS.
    // Addresses identify this active visit only and are never dereferenced.
    site: (usize, usize, usize),
    owner: ExecutionScope,
    function_depth: usize,
    root: String,
    key: String,
    name: String,
    stage: Stage,
}

#[derive(Clone)]
enum Stage {
    Seed,
    Prepared(Authority),
    Projected(Authority),
    Completed(Authority),
    Failed,
}

#[derive(Clone)]
struct Authority {
    site: (usize, usize, usize),
    binding: scopes::Binding,
    identity: usize,
    origin: ruff_text_size::TextSize,
    receiver: Type,
    field_owner: u32,
}

#[derive(Default)]
pub(super) struct MemberInputs {
    receipts: Vec<(MemberWrite, Authority)>,
    failed: bool,
}

impl Checker<'_> {
    pub(super) fn try_member_writer_seed(&self, statement: &Stmt) -> Option<Box<MemberWrite>> {
        let Stmt::Assign(assignment) = statement else {
            return None;
        };
        let [target @ Expr::Attribute(attribute)] = assignment.targets.as_slice() else {
            return None;
        };
        let Expr::Name(root) = attribute.value.as_ref() else {
            return None;
        };
        let owner = self.scopes.try_input_scope();
        if !attribute.ctx.is_store()
            || !root.ctx.is_load()
            || owner.1 != scopes::Kind::Function
            || owner.2.is_some()
            || self.functions.is_empty()
            || !matches!(assignment.value.as_ref(), Expr::NumberLiteral(number) if matches!(number.value, Number::Int(_)))
        {
            return None;
        }
        Some(Box::new(MemberWrite {
            site: (
                statement as *const Stmt as usize,
                target as *const Expr as usize,
                assignment.value.as_ref() as *const Expr as usize,
            ),
            owner,
            function_depth: self.functions.len(),
            root: root.id.to_string(),
            key: scopes::Scopes::attribute_key(target)?,
            name: attribute.attr.to_string(),
            stage: Stage::Seed,
        }))
    }

    fn current_try_member(&self, target: &Expr) -> Option<&MemberWrite> {
        let writer = self.try_inputs.writers.last()?;
        let member = writer.member.as_deref()?;
        let address = target as *const Expr as usize;
        (writer.target == Some(address)
            && member.site.1 == address
            && writer.statement == member.site.0
            && writer.owner == member.owner
            && writer.assignment.is_none()
            && writer.disposition
                == if matches!(&member.stage, Stage::Completed(_)) {
                    WriterDisposition::BinderCommit
                } else {
                    WriterDisposition::Unknown
                }
            && !writer.failed)
            .then_some(member)
    }

    fn fail_try_member(&mut self, target: &Expr) {
        let Some(writer) = self.try_inputs.writers.last_mut() else {
            return;
        };
        if writer.target != Some(target as *const Expr as usize) || writer.member.is_none() {
            return;
        }
        writer.member.as_mut().unwrap().stage = Stage::Failed;
        writer.failed = true;
        self.refuse_try_inputs();
    }

    fn int_member_fact(&self, ty: Type) -> bool {
        ty == Type::Int
            || matches!(
                self.context.node(ty).as_deref(),
                Some(
                    types::Node::Literal(types::Literal::Int(_))
                        | types::Node::LiteralValue(types::Literal::Int(_))
                )
            )
    }

    fn member_history_live(&self, member: &MemberWrite, receiver: Type) -> bool {
        let descendant = |key: &str| {
            key.strip_prefix(member.key.as_str())
                .is_some_and(|tail| tail.starts_with('.') || tail.starts_with('['))
        };
        let concrete = |value: &InputValue| matches!(value, InputValue::Concrete(_));
        let root_value = |value: &InputValue| *value == InputValue::Concrete(receiver);
        let own_value = |value: &InputValue| matches!(value, InputValue::Concrete(ty) if self.int_member_fact(*ty));
        !self.flow.deleted.contains(&member.root)
            && !self.flow.deleted.iter().any(|key| key.contains('\0'))
            && self.flow.reachable
            && self.try_inputs.checkpoints == 0
            && (!self.try_inputs.frames.is_empty() || !self.try_inputs.coverage.is_empty())
            && self.flow.narrowed.iter().all(|(key, &ty)| {
                !key.contains('\0')
                    && !descendant(key)
                    && (key != &member.key || self.int_member_fact(ty))
            })
            && self.try_inputs.coverage.iter().all(|watch| {
                !watch.refused
                    && watch.owner == member.owner
                    && watch.writer_depth <= self.try_inputs.writers.len()
            })
            && self.try_inputs.frames.iter().all(|frame| {
                !frame.refused
                    && frame.owner == member.owner
                    && frame.writer_depth <= self.try_inputs.writers.len()
                    && frame.inputs.count != 0
                    && !frame.inputs.deleted.contains(&member.root)
                    && !frame.inputs.deleted.iter().any(|key| key.contains('\0'))
                    && frame
                        .inputs
                        .baseline
                        .get(&member.root)
                        .is_some_and(root_value)
                    && frame.inputs.baseline.iter().all(|(key, value)| {
                        concrete(value)
                            && !key.contains('\0')
                            && !descendant(key)
                            && (key != &member.key || own_value(value))
                    })
                    && frame.inputs.options.iter().all(|(key, values)| {
                        !key.contains('\0')
                            && !descendant(key)
                            && values.as_ref().is_some_and(|values| {
                                !values.is_empty()
                                    && values.iter().all(|value| {
                                        concrete(value)
                                            && (key != &member.root || root_value(value))
                                            && (key != &member.key || own_value(value))
                                    })
                            })
                    })
            })
    }

    fn member_binding_live(&self, member: &MemberWrite, proof: &Authority) -> bool {
        proof.site == member.site
            && member.owner == self.scopes.try_input_scope()
            && member.owner.1 == scopes::Kind::Function
            && member.owner.2.is_none()
            && member.function_depth != 0
            && member.function_depth == self.functions.len()
            && self.scopes.direct_inferred_local_binding(&member.root)
                == Some((member.owner.0, proof.origin))
            && self.scopes.first_definition(&member.root) == Some(proof.origin)
            && self
                .scopes
                .inferred_declaration(&member.root, member.owner.0, proof.origin)
                == Some(proof.receiver)
            && self
                .scopes
                .resolved_binding(&member.root)
                .is_some_and(|binding| {
                    binding as *const scopes::Binding as usize == proof.identity
                        && binding == &proof.binding
                        && binding.module.is_none()
                })
            && self.environment().get(&member.root).copied() == Some(proof.receiver)
            && self.try_int_member_field_owner(&member.name, proof.receiver)
                == Some(proof.field_owner)
            && self.member_history_live(member, proof.receiver)
    }

    fn member_authority_live(
        &self,
        target: &Expr,
        member: &MemberWrite,
        proof: &Authority,
    ) -> bool {
        let Expr::Attribute(attribute) = target else {
            return false;
        };
        let Expr::Name(root) = attribute.value.as_ref() else {
            return false;
        };
        root.id.as_str() == member.root
            && attribute.attr.as_str() == member.name
            && attribute.ctx.is_store()
            && root.ctx.is_load()
            && scopes::Scopes::attribute_key(target).as_ref() == Some(&member.key)
            && proof.origin < root.start()
            && !self.scopes.unavailable_inferred_local(root)
            && self
                .context
                .observed_expression_type(&attribute.value)
                .is_none_or(|observed| observed == Some(proof.receiver))
            && self.member_binding_live(member, proof)
    }

    pub(super) fn retire_try_member_inputs(&mut self) {
        if self.try_inputs.frames.is_empty()
            && self.try_inputs.coverage.is_empty()
            && self.try_inputs.checkpoints == 0
        {
            // No old input or coverage consumer remains. Do not alter Flow or refusal.
            self.try_inputs.member_inputs = MemberInputs::default();
        }
    }

    pub(super) fn try_member_inputs_live(&self) -> bool {
        !self.try_inputs.member_inputs.failed
            && self
                .try_inputs
                .member_inputs
                .receipts
                .iter()
                .all(|(member, proof)| {
                    self.flow
                        .narrowed
                        .get(&member.key)
                        .is_some_and(|&ty| self.int_member_fact(ty))
                        && self.member_binding_live(member, proof)
                })
    }

    pub(in crate::infrastructure::mypy::checker) fn check_try_member_inputs(
        &mut self,
        target: Option<&Expr>,
    ) -> bool {
        self.retire_try_member_inputs();
        let root_store = self
            .try_inputs
            .member_inputs
            .receipts
            .iter()
            .any(|(member, _)| matches!(target, Some(Expr::Name(name)) if name.id.as_str() == member.root));
        let live = !root_store && self.try_member_inputs_live();
        if !live {
            // This failure survives writer pop and tentative checkpoint rollback.
            self.try_inputs.member_inputs.failed = true;
            self.refuse_try_inputs();
        }
        live
    }

    pub(super) fn prepare_try_member_writer(&mut self, target: &Expr, checked: Option<bool>) {
        let Some(member) = self.current_try_member(target).cloned() else {
            return;
        };
        let proof = (|| {
            if !matches!(&member.stage, Stage::Seed) || checked != Some(true) {
                return None;
            }
            let (owner, origin) = self.scopes.direct_inferred_local_binding(&member.root)?;
            let receiver = self
                .scopes
                .inferred_declaration(&member.root, owner, origin)?;
            let binding = self.scopes.resolved_binding(&member.root)?;
            let proof = Authority {
                site: member.site,
                identity: binding as *const scopes::Binding as usize,
                binding: binding.clone(),
                origin,
                receiver,
                field_owner: self.try_int_member_owner(target, receiver)?,
            };
            self.member_authority_live(target, &member, &proof)
                .then_some(proof)
        })();
        if let Some(proof) = proof {
            self.try_inputs
                .writers
                .last_mut()
                .unwrap()
                .member
                .as_mut()
                .unwrap()
                .stage = Stage::Prepared(proof);
        } else {
            self.fail_try_member(target);
        }
    }

    pub(in crate::infrastructure::mypy::checker) fn project_try_member_assignment(
        &mut self,
        target: &Expr,
        expected: Type,
        actual: Type,
    ) {
        let Some(member) = self.current_try_member(target).cloned() else {
            return;
        };
        let Stage::Prepared(proof) = &member.stage else {
            self.fail_try_member(target);
            return;
        };
        if expected != Type::Int
            || actual != Type::Int
            || self.flow.narrowed.contains_key(&member.key)
            || !self.member_authority_live(target, &member, proof)
        {
            self.fail_try_member(target);
            return;
        }
        // This is the actual post-store binder projection, never a type-equality repair.
        self.flow.narrowed.insert(member.key.clone(), Type::Int);
        self.try_inputs
            .writers
            .last_mut()
            .unwrap()
            .member
            .as_mut()
            .unwrap()
            .stage = Stage::Projected(proof.clone());
    }

    pub(super) fn complete_try_member_writer(
        &mut self,
        target: &Expr,
        actual: Option<Type>,
        accepted: bool,
    ) {
        let Some(member) = self.current_try_member(target).cloned() else {
            return;
        };
        let Stage::Projected(proof) = &member.stage else {
            self.fail_try_member(target);
            return;
        };
        if !accepted
            || actual != Some(Type::Int)
            || self.flow.narrowed.get(&member.key) != Some(&Type::Int)
            || !self.member_authority_live(target, &member, proof)
        {
            self.fail_try_member(target);
            return;
        }
        self.try_inputs
            .member_inputs
            .receipts
            .push((member.clone(), proof.clone()));
        let writer = self.try_inputs.writers.last_mut().unwrap();
        writer.member.as_mut().unwrap().stage = Stage::Completed(proof.clone());
        writer.disposition = WriterDisposition::BinderCommit;
    }

    pub(super) fn completed_try_member_writer(&self, target: &Expr) -> bool {
        let Some(member) = self.current_try_member(target) else {
            return false;
        };
        let Stage::Completed(proof) = &member.stage else {
            return false;
        };
        self.try_inputs.writers.last().is_some_and(|writer| {
            writer.disposition == WriterDisposition::BinderCommit
                && self.flow.narrowed.get(&member.key) == Some(&Type::Int)
                && self.member_authority_live(target, member, proof)
        })
    }
}
