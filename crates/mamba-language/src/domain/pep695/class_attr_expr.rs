use super::*;

/// Build `Class.Nested.attr` for a class-body lazy alias reference.
pub(super) fn class_attr_expr(path: &[Name], attr: &str, span: Span) -> Spanned<Expr> {
    let mut object = sp(Expr::Ident(path[0].clone()), span);
    for seg in &path[1..] {
        object = sp(
            Expr::Attr {
                object: Box::new(object),
                attr: seg.clone(),
            },
            span,
        );
    }
    sp(
        Expr::Attr {
            object: Box::new(object),
            attr: attr.to_string(),
        },
        span,
    )
}
