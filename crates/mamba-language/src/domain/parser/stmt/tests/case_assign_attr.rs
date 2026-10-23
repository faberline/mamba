use super::*;

// --- Assign to attribute ---

#[test]
pub(super) fn test_assign_attr() {
    match parse_stmt("a.b = 1\n") {
        Stmt::Assign { target, .. } => {
            assert!(matches!(target.node, Expr::Attr { .. }));
        }
        other => panic!("expected Assign(Attr), got {other:?}"),
    }
}
