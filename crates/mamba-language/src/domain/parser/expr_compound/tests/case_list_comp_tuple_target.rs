use super::*;

#[test]
pub(super) fn test_list_comp_tuple_target() {
    match parse_expr("[k for k, v in items]") {
        Expr::ListComp { generators, .. } => {
            assert_eq!(generators[0].targets, vec!["k", "v"]);
            assert!(generators[0].unpack_target);
        }
        other => panic!("expected ListComp, got {other:?}"),
    }
}
