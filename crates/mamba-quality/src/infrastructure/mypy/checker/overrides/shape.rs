//! Every call allowed by the parent must remain valid for the override.
use super::*;

pub(super) fn minimum_positional(signature: &Signature) -> usize {
    signature
        .parameters
        .iter()
        .filter(|parameter| {
            parameter.required
                && matches!(
                    parameter.kind,
                    Kind::PositionalOnly | Kind::PositionalOrKeyword
                )
        })
        .count()
}

pub(super) fn compatible(context: &types::Context, parent: &Signature, child: &Signature) -> bool {
    context.accepts_callable(parent, child, true)
}
