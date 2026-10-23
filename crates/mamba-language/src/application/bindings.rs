use mamba_shared_kernel::ast::{Expr, Stmt};
use mamba_shared_kernel::source::span::Spanned;

pub fn collect_assignment_targets(
    stmts: &[Spanned<Stmt>],
    assigned: &mut Vec<String>,
    declared: &mut Vec<String>,
) {
    crate::domain::bindings::collect_assignment_targets(stmts, assigned, declared);
}
pub fn collect_walrus_targets(expr: &Expr, assigned: &mut Vec<String>) {
    crate::domain::bindings::collect_walrus_targets(expr, assigned);
}
pub fn collect_walrus_targets_in_stmts(stmts: &[Spanned<Stmt>], assigned: &mut Vec<String>) {
    crate::domain::bindings::collect_walrus_targets_in_stmts(stmts, assigned);
}
