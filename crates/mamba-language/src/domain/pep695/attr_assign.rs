use super::*;

pub(super) fn attr_assign(
    path: &[Name],
    attr: &str,
    value: Spanned<Expr>,
    span: Span,
) -> Spanned<Stmt> {
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
    Spanned::new(
        Stmt::Assign {
            target: sp(
                Expr::Attr {
                    object: Box::new(object),
                    attr: attr.to_string(),
                },
                span,
            ),
            value,
        },
        span,
    )
}
