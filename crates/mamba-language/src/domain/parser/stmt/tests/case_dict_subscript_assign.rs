use super::*;

#[test]
pub(super) fn test_dict_subscript_assign() {
    // Dict literal subscript assignment
    match parse_stmt("d = {}['x']\n") {
        Stmt::Assign { value, .. } => {
            assert!(matches!(value.node, Expr::Index { .. }));
        }
        other => panic!("expected Assign, got {other:?}"),
    }
}
