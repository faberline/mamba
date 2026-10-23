use super::*;

// --- If ---

#[test]
pub(super) fn test_if_basic() {
    match parse_stmt("if True:\n    pass\n") {
        Stmt::If {
            condition,
            body,
            elif_clauses,
            else_body,
        } => {
            assert!(matches!(condition.node, Expr::BoolLit(true)));
            assert_eq!(body.len(), 1);
            assert!(elif_clauses.is_empty());
            assert!(else_body.is_none());
        }
        other => panic!("expected If, got {other:?}"),
    }
}
