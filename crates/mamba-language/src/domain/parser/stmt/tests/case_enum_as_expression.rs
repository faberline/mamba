use super::*;

#[test]
pub(super) fn test_enum_as_expression() {
    match parse_stmt("enum = 1\n") {
        Stmt::Assign { target, .. } => {
            assert!(matches!(target.node, Expr::Ident(ref n) if n == "enum"));
        }
        other => panic!("expected Assign, got {other:?}"),
    }
}
