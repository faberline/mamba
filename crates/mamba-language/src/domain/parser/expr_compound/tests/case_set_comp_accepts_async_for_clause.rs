use super::*;

#[test]
pub(super) fn test_set_comp_accepts_async_for_clause() {
    match parse_expr("{x async for x in items}") {
        Expr::SetComp { generators, .. } => {
            assert_eq!(generators.len(), 1);
            assert!(generators[0].is_async);
        }
        other => panic!("expected SetComp, got {other:?}"),
    }
}
