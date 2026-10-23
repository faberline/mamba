use super::*;

pub(super) fn save_binding(binding: &SavedBinding, span: Span) -> Spanned<Stmt> {
    Spanned::new(
        Stmt::Try {
            body: vec![
                assign_ident(
                    &binding.saved_name,
                    sp(Expr::Ident(binding.name.clone()), span),
                    span,
                ),
                assign_ident(&binding.had_name, sp(Expr::BoolLit(true), span), span),
            ],
            handlers: vec![ExceptHandler {
                exc_type: Some(sp(Expr::Ident("NameError".to_string()), span)),
                name: None,
                body: vec![assign_ident(
                    &binding.had_name,
                    sp(Expr::BoolLit(false), span),
                    span,
                )],
                is_star: false,
                span,
            }],
            else_body: None,
            finally_body: None,
        },
        span,
    )
}
