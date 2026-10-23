use super::*;

// --- Walrus ---

#[test]
pub(super) fn test_walrus() {
    match parse_expr("(x := 10)") {
        Expr::Walrus { target, value } => {
            assert_eq!(target, "x");
            assert!(matches!(value.node, Expr::IntLit(10)));
        }
        other => panic!("expected Walrus, got {other:?}"),
    }
}
