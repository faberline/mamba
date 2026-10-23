use super::*;

#[test]
pub(super) fn test_list_display_assignment_target_normalized_to_tuple() {
    // `[*a, b] = xs` — the list display on the LHS is a sequence-unpack
    // target and must be rewritten to a TupleLit so the existing
    // tuple/unpack lowering (which understands Starred) drives the unpack.
    match parse_stmt("[*a, b] = xs\n") {
        Stmt::Assign { target, .. } => match target.node {
            Expr::TupleLit(elems) => {
                assert_eq!(elems.len(), 2);
                assert!(matches!(&elems[0].node, Expr::Starred(_)));
                assert!(matches!(&elems[1].node, Expr::Ident(n) if n == "b"));
            }
            other => panic!("expected TupleLit target, got {other:?}"),
        },
        other => panic!("expected Assign, got {other:?}"),
    }
}
