use super::*;

#[test]
pub(super) fn test_set_comp() {
    match parse_expr("{x for x in items}") {
        Expr::SetComp { generators, .. } => {
            assert_eq!(generators.len(), 1);
        }
        other => panic!("expected SetComp, got {other:?}"),
    }
}
