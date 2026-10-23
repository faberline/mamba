use super::*;

#[test]
pub(super) fn test_chained_attr() {
    match parse_expr_str("a.b.c") {
        Expr::Attr { object, attr } => {
            assert_eq!(attr, "c");
            assert!(matches!(object.node, Expr::Attr { .. }));
        }
        other => panic!("expected chained Attr, got {other:?}"),
    }
}
