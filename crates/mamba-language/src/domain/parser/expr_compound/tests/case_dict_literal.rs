use super::*;

#[test]
pub(super) fn test_dict_literal() {
    match parse_expr("{1: 2, 3: 4}") {
        Expr::DictLit(pairs) => {
            assert_eq!(pairs.len(), 2);
        }
        other => panic!("expected DictLit, got {other:?}"),
    }
}
