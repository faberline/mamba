use super::*;

#[test]
pub(super) fn test_list_comp_complex_target_collects_binding_names() {
    match parse_expr("[x for a, (*b, c[d+e::f(g)], h.i) in items]") {
        Expr::ListComp { generators, .. } => {
            assert_eq!(generators[0].targets, vec!["a", "b"]);
            assert!(generators[0].unpack_target);
        }
        other => panic!("expected ListComp, got {other:?}"),
    }
}
