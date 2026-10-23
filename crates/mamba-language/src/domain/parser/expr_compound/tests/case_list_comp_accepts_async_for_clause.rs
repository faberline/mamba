use super::*;

#[test]
pub(super) fn test_list_comp_accepts_async_for_clause() {
    let src = "[x async for x in aiter]\n";
    match parse_expr(src) {
        Expr::ListComp { generators, .. } => {
            assert_eq!(generators.len(), 1);
            assert!(generators[0].is_async);
        }
        other => panic!("expected ListComp, got {other:?}"),
    }
}
