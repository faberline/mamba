use super::*;

// --- Literal patterns ---

#[test]
pub(super) fn test_int_literal_pattern() {
    match parse_pattern("42") {
        Pattern::Literal(Expr::IntLit(42)) => {}
        other => panic!("expected Literal(42), got {other:?}"),
    }
}
