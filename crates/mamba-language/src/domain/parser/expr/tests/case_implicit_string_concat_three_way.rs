use super::*;

#[test]
pub(super) fn test_implicit_string_concat_three_way() {
    match parse_expr_str("\"a\" \"b\" \"c\"") {
        Expr::StrLit(s) => assert_eq!(s, "abc"),
        other => panic!("expected StrLit, got {other:?}"),
    }
}
