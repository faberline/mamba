use super::*;

#[test]
pub(super) fn test_return_value() {
    match parse_stmt("return 42\n") {
        Stmt::Return(Some(e)) => {
            assert!(matches!(e.node, Expr::IntLit(42)));
        }
        other => panic!("expected Return(Some(42)), got {other:?}"),
    }
}
