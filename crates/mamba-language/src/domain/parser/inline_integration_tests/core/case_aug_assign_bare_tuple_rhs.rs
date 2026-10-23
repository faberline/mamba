use super::*;

#[test]
pub(super) fn test_aug_assign_bare_tuple_rhs() {
    // `x += a, b` — bare-tuple form (no parens) on aug-assign RHS.
    let module = parse(
        "x = ()\n\
         x += 1, 2\n",
    );
    match &module.stmts[1].node {
        Stmt::AugAssign { value, .. } => match &value.node {
            Expr::TupleLit(elems) => assert_eq!(elems.len(), 2),
            other => panic!("expected TupleLit RHS, got {other:?}"),
        },
        other => panic!("expected AugAssign, got {other:?}"),
    }
}
