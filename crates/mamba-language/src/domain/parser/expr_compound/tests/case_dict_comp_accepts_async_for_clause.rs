use super::*;

#[test]
pub(super) fn test_dict_comp_accepts_async_for_clause() {
    match parse_expr("{k: v async for k, v in items}") {
        Expr::DictComp { generators, .. } => {
            assert_eq!(generators.len(), 1);
            assert!(generators[0].is_async);
        }
        other => panic!("expected DictComp, got {other:?}"),
    }
}
