use super::*;

// --- Attribute access ---

#[test]
pub(super) fn test_attr_access() {
    match parse_expr_str("a.b") {
        Expr::Attr { object, attr } => {
            assert!(matches!(object.node, Expr::Ident(ref n) if n == "a"));
            assert_eq!(attr, "b");
        }
        other => panic!("expected Attr, got {other:?}"),
    }
}
