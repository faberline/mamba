//! Metadata reads keep the assignments visible at each source position.
use super::{resolution::Context, Checker};
use mamba_language::application::quality_syntax::python_ast::Expr;
use ruff_text_size::Ranged;
use std::collections::BTreeSet;

type Active = BTreeSet<(Context, u32, u32)>;

impl Checker<'_> {
    pub(super) fn metadata_nodes(
        &self,
        expression: &Expr,
        context: Context,
    ) -> Vec<(Expr, Context)> {
        self.metadata_nodes_inner(expression, context, &mut Active::new())
    }

    fn metadata_nodes_inner(
        &self,
        expression: &Expr,
        context: Context,
        active: &mut Active,
    ) -> Vec<(Expr, Context)> {
        let key = (
            context.clone(),
            expression.start().to_u32(),
            expression.end().to_u32(),
        );
        if !active.insert(key.clone()) {
            return Vec::new();
        }
        let result = if let (Expr::Name(name), Context::Scope(index)) = (expression, &context) {
            if let Some((owner, values)) =
                self.scopes
                    .values_before(name.id.as_str(), *index, name.start())
            {
                values
                    .iter()
                    .flat_map(|value| {
                        self.metadata_nodes_inner(value, Context::Scope(owner), active)
                    })
                    .collect()
            } else {
                self.metadata_slot_nodes(expression, context, active)
            }
        } else {
            self.metadata_slot_nodes(expression, context, active)
        };
        active.remove(&key);
        result
    }

    fn metadata_slot_nodes(
        &self,
        expression: &Expr,
        context: Context,
        active: &mut Active,
    ) -> Vec<(Expr, Context)> {
        self.slot_nodes(expression, context.clone())
            .into_iter()
            .flat_map(|(value, origin)| {
                if value.range() == expression.range() && origin == context {
                    vec![(value, origin)]
                } else {
                    self.metadata_nodes_inner(&value, origin, active)
                }
            })
            .collect()
    }
}
