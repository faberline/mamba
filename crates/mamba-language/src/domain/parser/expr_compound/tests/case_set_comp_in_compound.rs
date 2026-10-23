use super::*;

/// Parse set comprehension `{x for x in items}`.
#[test]
pub(super) fn test_set_comp_in_compound() {
    match parse_expr("{x for x in items}") {
        Expr::SetComp { .. } => {}
        other => panic!("expected SetComp, got {other:?}"),
    }
}
