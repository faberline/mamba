use super::*;

// --- Assignment ---

#[test]
pub(super) fn test_simple_assign() {
    match parse_stmt("x = 1\n") {
        Stmt::Assign { target, value } => {
            assert!(matches!(target.node, Expr::Ident(ref n) if n == "x"));
            assert!(matches!(value.node, Expr::IntLit(1)));
        }
        other => panic!("expected Assign, got {other:?}"),
    }
}
