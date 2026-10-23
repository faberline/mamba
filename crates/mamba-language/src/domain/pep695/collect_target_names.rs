use super::*;

pub(super) fn collect_target_names(expr: &Expr, out: &mut HashSet<Name>) {
    match expr {
        Expr::Ident(name) => {
            out.insert(name.clone());
        }
        Expr::TupleLit(items) | Expr::ListLit(items) | Expr::UnpackTarget(items) => {
            for item in items {
                collect_target_names(&item.node, out);
            }
        }
        Expr::Starred(inner) => collect_target_names(&inner.node, out),
        _ => {}
    }
}
