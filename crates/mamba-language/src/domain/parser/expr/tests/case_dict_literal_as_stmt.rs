use super::*;

/// Parsing `{'a': 1, 'b': 2}` as an expression statement should produce a DictLit.
#[test]
pub(super) fn test_dict_literal_as_stmt() {
    match parse_expr_str("{'a': 1, 'b': 2}") {
        Expr::DictLit(entries) => {
            assert_eq!(entries.len(), 2, "dict literal should have 2 entries");
        }
        other => panic!("expected DictLit, got {other:?}"),
    }
}
