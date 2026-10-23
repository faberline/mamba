use super::*;

pub(super) fn assign_ident(name: &str, value: Spanned<Expr>, span: Span) -> Spanned<Stmt> {
    Spanned::new(
        Stmt::Assign {
            target: sp(Expr::Ident(name.to_string()), span),
            value,
        },
        span,
    )
}
