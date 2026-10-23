//! Actual target-free protocol checks complete the parent before its body runs.
use super::*;
use quality_syntax::python_ast::StmtWith;

#[derive(Clone)]
pub(super) struct WithItems {
    site: usize,
    expressions: Vec<usize>,
    function_depth: usize,
    next: usize,
}

impl Checker<'_> {
    pub(super) fn try_with_items_seed(&self, statement: &Stmt) -> Option<WithItems> {
        let Stmt::With(with) = statement else {
            return None;
        };
        let owner = self.scopes.try_input_scope();
        if with.is_async
            || with.items.is_empty()
            || with.items.iter().any(|item| item.optional_vars.is_some())
            || owner.1 != scopes::Kind::Function
            || owner.2.is_some()
            || self.functions.is_empty()
        {
            return None;
        }
        Some(WithItems {
            site: with as *const StmtWith as usize,
            expressions: with
                .items
                .iter()
                .map(|item| &item.context_expr as *const Expr as usize)
                .collect(),
            function_depth: self.functions.len(),
            next: 0,
        })
    }

    pub(in crate::infrastructure::mypy::checker) fn try_with_item_active(
        &self,
        with: &StmtWith,
        context: &Expr,
    ) -> bool {
        self.try_inputs.writers.last().is_some_and(|writer| {
            writer.with_items.as_ref().is_some_and(|proof| {
                !writer.failed
                    && proof.site == with as *const StmtWith as usize
                    && writer.owner == self.scopes.try_input_scope()
                    && proof.function_depth == self.functions.len()
                    && proof.expressions.get(proof.next) == Some(&(context as *const Expr as usize))
            })
        })
    }

    pub(in crate::infrastructure::mypy::checker) fn complete_try_with_item(
        &mut self,
        with: &StmtWith,
        context: &Expr,
        proved: bool,
    ) -> bool {
        let live = proved && self.try_with_item_active(with, context);
        let Some(writer) = self.try_inputs.writers.last_mut() else {
            return false;
        };
        let Some(proof) = writer
            .with_items
            .as_mut()
            .filter(|proof| proof.site == with as *const StmtWith as usize)
        else {
            return false;
        };
        if !live {
            writer.failed = true;
            self.refuse_try_inputs();
            return false;
        }
        proof.next += 1;
        if proof.next == proof.expressions.len() {
            writer.disposition = WriterDisposition::NoBinderEvent;
        }
        true
    }
}
