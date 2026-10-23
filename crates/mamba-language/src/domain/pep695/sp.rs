use super::*;

pub(super) fn sp(node: Expr, span: Span) -> Spanned<Expr> {
    Spanned::new(node, span)
}
