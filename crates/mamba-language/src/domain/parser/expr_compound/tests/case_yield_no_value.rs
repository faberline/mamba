use super::*;

// --- Yield ---

#[test]
pub(super) fn test_yield_no_value() {
    // yield at end of line
    match parse_expr("yield") {
        Expr::Yield(None) => {}
        other => panic!("expected Yield(None), got {other:?}"),
    }
}
