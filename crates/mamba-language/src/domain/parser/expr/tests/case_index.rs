use super::*;

// --- Index / Slice ---

#[test]
pub(super) fn test_index() {
    match parse_expr_str("a[0]") {
        Expr::Index { object, index } => {
            assert!(matches!(object.node, Expr::Ident(ref n) if n == "a"));
            assert!(matches!(index.node, Expr::IntLit(0)));
        }
        other => panic!("expected Index, got {other:?}"),
    }
}
