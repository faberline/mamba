use super::*;

#[test]
pub(super) fn test_string_literal_pattern() {
    match parse_pattern("\"hello\"") {
        Pattern::Literal(Expr::StrLit(s)) => assert_eq!(s, "hello"),
        other => panic!("expected Literal(str), got {other:?}"),
    }
}
