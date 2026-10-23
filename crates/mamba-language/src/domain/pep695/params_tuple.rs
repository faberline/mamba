use super::*;

/// Build a `(P1, P2, ...)` tuple expression referencing the typevar bindings.
pub(super) fn params_tuple(binding_names: &[Name], span: Span) -> Spanned<Expr> {
    sp(
        Expr::TupleLit(
            binding_names
                .iter()
                .map(|name| sp(Expr::Ident(name.clone()), span))
                .collect(),
        ),
        span,
    )
}
