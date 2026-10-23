use super::*;

// --- Assign to index ---

#[test]
pub(super) fn test_assign_index() {
    match parse_stmt("a[0] = 1\n") {
        Stmt::Assign { target, .. } => {
            assert!(matches!(target.node, Expr::Index { .. }));
        }
        other => panic!("expected Assign(Index), got {other:?}"),
    }
}
