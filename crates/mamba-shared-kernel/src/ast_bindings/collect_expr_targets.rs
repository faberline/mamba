use super::*;

/// Recursive expression walk for identifier-targets inside tuple / unpack
/// assignments.
pub(super) fn collect_expr_targets(expr: &Expr, assigned: &mut Vec<String>) {
    match expr {
        Expr::Ident(name) => {
            if !assigned.iter().any(|n| n == name) {
                assigned.push(name.clone());
            }
        }
        Expr::TupleLit(elems) | Expr::UnpackTarget(elems) => {
            for e in elems {
                collect_expr_targets(&e.node, assigned);
            }
        }
        Expr::Starred(inner) => {
            collect_expr_targets(&inner.node, assigned);
        }
        _ => {}
    }
}
