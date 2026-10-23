use super::*;

/// Normalize an expression that appears in assignment-target position.
///
/// Python allows a list display on the left of `=` as a sequence-unpacking
/// target: `[*a, b] = xs` and `[a, b] = xs` mean the same as `(*a, b) = xs`
/// / `(a, b) = xs`.  The parser produces a `ListLit` for `[...]`; rewrite it
/// to a `TupleLit` so the existing tuple/unpack lowering (which already
/// handles `Starred` elements) drives the unpack.  Recurses into nested
/// targets (`[a, [b, c]] = ...`) and through `Starred` so a starred list
/// element (`[*[x, y], z]`) is also normalized.  Non-sequence targets
/// (`Ident`, `Index`, `Attr`, ...) are returned unchanged.
impl<'a> Parser<'a> {
    pub(super) fn normalize_assign_target(target: Spanned<Expr>) -> Spanned<Expr> {
        let span = target.span;
        match target.node {
            Expr::ListLit(elems) => {
                let elems = elems
                    .into_iter()
                    .map(Self::normalize_assign_target)
                    .collect();
                Spanned::new(Expr::TupleLit(elems), span)
            }
            Expr::TupleLit(elems) => {
                let elems = elems
                    .into_iter()
                    .map(Self::normalize_assign_target)
                    .collect();
                Spanned::new(Expr::TupleLit(elems), span)
            }
            Expr::Starred(inner) => {
                let inner = Self::normalize_assign_target(*inner);
                Spanned::new(Expr::Starred(Box::new(inner)), span)
            }
            other => Spanned::new(other, span),
        }
    }
}
