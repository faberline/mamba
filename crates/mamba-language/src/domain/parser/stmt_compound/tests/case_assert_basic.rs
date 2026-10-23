use super::*;

// --- Assert ---

#[test]
pub(super) fn test_assert_basic() {
    match parse_stmt("assert True\n") {
        Stmt::Assert { test, msg } => {
            assert!(matches!(test.node, Expr::BoolLit(true)));
            assert!(msg.is_none());
        }
        other => panic!("expected Assert, got {other:?}"),
    }
}
