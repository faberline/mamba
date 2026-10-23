//! Each real composite loop bind must prove all actual leaves again.
use super::*;

#[derive(Clone)]
pub(super) struct ForTarget {
    leaves: Vec<usize>,
    function_depth: usize,
    next: Option<usize>,
}

impl ForTarget {
    fn leaves(target: &Expr, leaves: &mut Vec<usize>, names: &mut BTreeSet<String>) -> bool {
        let children: &[Expr] = match target {
            Expr::Name(name) => {
                if !name.ctx.is_store() || !names.insert(name.id.to_string()) {
                    return false;
                }
                leaves.push(target as *const Expr as usize);
                return true;
            }
            Expr::Tuple(tuple) if tuple.ctx.is_store() => &tuple.elts,
            Expr::List(list) if list.ctx.is_store() => &list.elts,
            _ => return false,
        };
        !children.is_empty()
            && children
                .iter()
                .all(|child| Self::leaves(child, leaves, names))
    }
}

impl Checker<'_> {
    pub(super) fn try_for_target_seed(&self, statement: &Stmt) -> Option<ForTarget> {
        let Stmt::For(looped) = statement else {
            return None;
        };
        if looped.is_async || !matches!(looped.target.as_ref(), Expr::Tuple(_) | Expr::List(_)) {
            return None;
        }
        let owner = self.scopes.try_input_scope();
        if owner.1 != scopes::Kind::Function || owner.2.is_some() || self.functions.is_empty() {
            return None;
        }
        let mut leaves = Vec::new();
        if !ForTarget::leaves(&looped.target, &mut leaves, &mut BTreeSet::new()) {
            return None;
        }
        Some(ForTarget {
            leaves,
            function_depth: self.functions.len(),
            next: None,
        })
    }

    pub(in crate::infrastructure::mypy::checker) fn begin_try_for_target_pass(
        &mut self,
        target: &Expr,
        item_known: bool,
    ) -> Option<usize> {
        let depth = self.try_inputs.writers.len().checked_sub(1)?;
        let writer = &mut self.try_inputs.writers[depth];
        let proof = writer.for_target.as_mut()?;
        let valid = writer.target == Some(target as *const Expr as usize)
            && writer.owner == self.scopes.try_input_scope()
            && proof.function_depth == self.functions.len()
            && proof.next.is_none()
            && item_known
            && !writer.failed;
        // A previous real pass cannot stand in for this invocation.
        writer.disposition = WriterDisposition::Unknown;
        proof.next = Some(0);
        if !valid {
            writer.failed = true;
            self.refuse_try_inputs();
        }
        Some(depth)
    }

    pub(in crate::infrastructure::mypy::checker) fn finish_try_for_target_pass(
        &mut self,
        target: &Expr,
        pass: Option<usize>,
    ) {
        let Some(depth) = pass else {
            return;
        };
        if self.try_inputs.writers.len() != depth + 1 {
            self.refuse_try_inputs();
            return;
        }
        let writer = &mut self.try_inputs.writers[depth];
        let Some(proof) = writer.for_target.as_mut() else {
            self.refuse_try_inputs();
            return;
        };
        let complete = writer.target == Some(target as *const Expr as usize)
            && writer.owner == self.scopes.try_input_scope()
            && proof.function_depth == self.functions.len()
            && proof.next.take() == Some(proof.leaves.len())
            && !writer.failed;
        if complete {
            writer.disposition = WriterDisposition::NoBinderEvent;
        } else {
            writer.failed = true;
            self.refuse_try_inputs();
        }
    }

    fn complete_composite_for_leaf(&mut self, target: &Expr, proved: bool) -> Option<bool> {
        let writer = self.try_inputs.writers.last_mut()?;
        let proof = writer.for_target.as_mut()?;
        if proved
            && !writer.failed
            && writer.owner == self.scopes.try_input_scope()
            && proof.function_depth == self.functions.len()
            && proof.next.is_some_and(|next| {
                proof.leaves.get(next) == Some(&(target as *const Expr as usize))
            })
        {
            *proof.next.as_mut().expect("active composite target pass") += 1;
            return Some(true);
        }
        writer.failed = true;
        self.refuse_try_inputs();
        Some(false)
    }

    pub(in crate::infrastructure::mypy::checker) fn complete_try_for_target(
        &mut self,
        target: &Expr,
        proved: bool,
    ) -> bool {
        if let Some(completed) = self.complete_composite_for_leaf(target, proved) {
            return completed;
        }
        let Some(writer) = self
            .try_inputs
            .writers
            .last_mut()
            .filter(|writer| writer.target == Some(target as *const Expr as usize))
        else {
            return false;
        };
        if !proved || writer.owner != self.scopes.try_input_scope() || writer.failed {
            // Real loop passes share this failure. Only an existing tentative
            // writer checkpoint may roll it back with the rest of the proof.
            writer.failed = true;
            self.refuse_try_inputs();
            return false;
        }
        writer.disposition = WriterDisposition::NoBinderEvent;
        true
    }
}
