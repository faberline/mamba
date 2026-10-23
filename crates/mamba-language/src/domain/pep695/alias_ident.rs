use super::*;

pub(super) fn alias_ident(target: &str, source: &str, span: Span) -> Spanned<Stmt> {
    Spanned::new(
        Stmt::Assign {
            target: sp(Expr::Ident(target.to_string()), span),
            value: sp(Expr::Ident(source.to_string()), span),
        },
        span,
    )
}
