use super::*;

#[test]
pub(super) fn test_try_bare_empty_dict_in_body() {
    // Bare empty dict expression statement in try body
    match parse_stmt("try:\n    {}\nexcept:\n    pass\n") {
        Stmt::Try { body, handlers, .. } => {
            assert_eq!(body.len(), 1);
            match &body[0].node {
                Stmt::ExprStmt(expr) => {
                    assert!(matches!(&expr.node, Expr::DictLit(entries) if entries.is_empty()));
                }
                other => panic!("expected ExprStmt(DictLit), got {other:?}"),
            }
            assert_eq!(handlers.len(), 1);
        }
        other => panic!("expected Try, got {other:?}"),
    }
}
