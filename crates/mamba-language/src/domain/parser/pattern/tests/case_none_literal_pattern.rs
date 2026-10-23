use super::*;

#[test]
pub(super) fn test_none_literal_pattern() {
    match parse_pattern("None") {
        Pattern::Literal(Expr::NoneLit) => {}
        other => panic!("expected Literal(None), got {other:?}"),
    }
}
