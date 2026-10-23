use super::*;

#[test]
pub(super) fn test_false_literal_pattern() {
    match parse_pattern("False") {
        Pattern::Literal(Expr::BoolLit(false)) => {}
        other => panic!("expected Literal(False), got {other:?}"),
    }
}
