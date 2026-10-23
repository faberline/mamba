use super::*;

#[test]
pub(super) fn test_list_comp_parenthesized_singleton_tuple_target() {
    match parse_expr("[v for (v,) in items]") {
        Expr::ListComp { generators, .. } => {
            assert_eq!(generators[0].targets, vec!["v"]);
            assert!(generators[0].unpack_target);
        }
        other => panic!("expected ListComp, got {other:?}"),
    }
}
