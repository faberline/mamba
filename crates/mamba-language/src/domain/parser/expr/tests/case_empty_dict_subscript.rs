use super::*;

/// Parsing `{}['x']` should parse as an index operation on empty dict.
#[test]
pub(super) fn test_empty_dict_subscript() {
    match parse_expr_str("{}['x']") {
        Expr::Index { object, index } => {
            assert!(
                matches!(object.node, Expr::DictLit(ref e) if e.is_empty()),
                "index object should be empty DictLit"
            );
            assert!(
                matches!(index.node, Expr::StrLit(ref s) if s == "x"),
                "index should be 'x'"
            );
        }
        other => panic!("expected Index on DictLit, got {other:?}"),
    }
}
