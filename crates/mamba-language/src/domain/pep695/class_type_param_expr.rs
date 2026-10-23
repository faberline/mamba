use super::*;

pub(super) fn class_type_param_expr(path: &[Name], index: usize, span: Span) -> Spanned<Expr> {
    sp(
        Expr::Index {
            object: Box::new(class_attr_expr(path, "__type_params__", span)),
            index: Box::new(sp(Expr::IntLit(index as i64), span)),
        },
        span,
    )
}
