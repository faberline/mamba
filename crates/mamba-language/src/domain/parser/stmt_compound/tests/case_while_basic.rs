use super::*;

// --- While ---

#[test]
pub(super) fn test_while_basic() {
    match parse_stmt("while True:\n    pass\n") {
        Stmt::While {
            condition,
            body,
            else_body,
        } => {
            assert!(matches!(condition.node, Expr::BoolLit(true)));
            assert_eq!(body.len(), 1);
            assert!(else_body.is_none());
        }
        other => panic!("expected While, got {other:?}"),
    }
}
