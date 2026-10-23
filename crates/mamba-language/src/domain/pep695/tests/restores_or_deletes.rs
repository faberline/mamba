use super::*;

pub(super) fn restores_or_deletes(stmts: &[Spanned<Stmt>], name: &str) -> bool {
    stmts.iter().any(|stmt| {
        let Stmt::If {
            body,
            else_body: Some(else_body),
            ..
        } = &stmt.node
        else {
            return false;
        };
        body.iter().any(|stmt| {
            matches!(
                &stmt.node,
                Stmt::Assign { target, .. }
                    if matches!(&target.node, Expr::Ident(target) if target == name)
            )
        }) && else_body.iter().any(|stmt| {
            matches!(
                &stmt.node,
                Stmt::Del(target)
                    if matches!(&target.node, Expr::Ident(target) if target == name)
            )
        })
    })
}
