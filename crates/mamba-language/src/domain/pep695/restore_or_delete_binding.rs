use super::*;

pub(super) fn restore_or_delete_binding(binding: &SavedBinding, span: Span) -> Vec<Spanned<Stmt>> {
    vec![
        Spanned::new(
            Stmt::If {
                condition: sp(Expr::Ident(binding.had_name.clone()), span),
                body: vec![
                    assign_ident(
                        &binding.name,
                        sp(Expr::Ident(binding.saved_name.clone()), span),
                        span,
                    ),
                    del_ident(&binding.saved_name, span),
                ],
                elif_clauses: Vec::new(),
                else_body: Some(vec![del_ident(&binding.name, span)]),
            },
            span,
        ),
        del_ident(&binding.had_name, span),
    ]
}
