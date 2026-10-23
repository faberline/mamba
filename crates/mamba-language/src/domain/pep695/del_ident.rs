use super::*;

pub(super) fn del_ident(name: &str, span: Span) -> Spanned<Stmt> {
    Spanned::new(Stmt::Del(sp(Expr::Ident(name.to_string()), span)), span)
}
