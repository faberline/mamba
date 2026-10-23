use super::*;

#[test]
pub(super) fn test_if_dict_assignment_with_followup() {
    // S-TRY-DICT-4: dict in if body followed by another statement
    match parse_stmt("if True:\n    d = {}\n    print(d)\n") {
        Stmt::If { body, .. } => {
            assert_eq!(body.len(), 2);
            match &body[0].node {
                Stmt::Assign { target, value } => {
                    assert!(matches!(&target.node, Expr::Ident(n) if n == "d"));
                    assert!(matches!(&value.node, Expr::DictLit(entries) if entries.is_empty()));
                }
                other => panic!("expected Assign, got {other:?}"),
            }
            match &body[1].node {
                Stmt::ExprStmt(expr) => {
                    assert!(matches!(&expr.node, Expr::Call { .. }));
                }
                other => panic!("expected ExprStmt(Call), got {other:?}"),
            }
        }
        other => panic!("expected If, got {other:?}"),
    }
}
