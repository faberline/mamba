use super::*;

pub(super) fn collect_runtime_type_param_uses_block(
    stmts: &[Spanned<Stmt>],
    type_param_names: &HashSet<Name>,
    out: &mut HashSet<Name>,
    shadowed: &mut HashSet<Name>,
) {
    for stmt in stmts {
        collect_runtime_type_param_uses_stmt(stmt, type_param_names, out, shadowed);
    }
}
