use super::*;

#[test]
pub(super) fn test_list_comp_with_condition() {
    match parse_expr("[x for x in items if x > 0]") {
        Expr::ListComp { generators, .. } => {
            assert_eq!(generators[0].conditions.len(), 1);
        }
        other => panic!("expected ListComp, got {other:?}"),
    }
}
