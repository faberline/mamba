use super::*;

// --- Chained assignment ---

#[test]
pub(super) fn test_chained_assign() {
    // `a = b = 1` desugars into `__chained_N = 1; b = __chained_N; a = __chained_N`
    // (CPython-shared-RHS semantics). parse_stmt returns the first
    // statement of the desugared chain — the temp assignment.
    match parse_stmt("a = b = 1\n") {
        Stmt::Assign { target, value } => {
            assert!(
                matches!(target.node, Expr::Ident(ref n) if n.starts_with("__chained_")),
                "target should be the desugared __chained_N temp",
            );
            assert!(matches!(value.node, Expr::IntLit(1)));
        }
        other => panic!("expected Assign, got {other:?}"),
    }
}
