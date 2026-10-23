use super::*;

pub(super) fn find_typevar_assign<'a>(
    stmts: &'a [Spanned<Stmt>],
    display_name: &str,
) -> Option<(&'a Spanned<Expr>, &'a [CallArg])> {
    stmts.iter().find_map(|stmt| {
        let Stmt::Assign { target, value } = &stmt.node else {
            return None;
        };
        let Expr::Call { func, args } = &value.node else {
            return None;
        };
        if !matches!(&func.node, Expr::Ident(name) if name == TYPEVAR_INTRINSIC)
            || !matches!(
                args.first(),
                Some(CallArg::Positional(arg))
                    if matches!(&arg.node, Expr::StrLit(name) if name == display_name)
            )
        {
            return None;
        }
        Some((target, args.as_slice()))
    })
}
