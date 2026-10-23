use super::*;

#[test]
pub(super) fn test_true_literal_pattern() {
    match parse_pattern("True") {
        Pattern::Literal(Expr::BoolLit(true)) => {}
        other => panic!("expected Literal(True), got {other:?}"),
    }
}
