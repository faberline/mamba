use super::*;

pub(super) fn mangle_stmts(stmts: &mut [Spanned<Stmt>], class: Option<&str>) {
    for s in stmts.iter_mut() {
        mangle_stmt(s, class);
    }
}
