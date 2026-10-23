use super::*;

#[test]
pub(super) fn test_aug_assign_trailing_comma_tuple_rhs() {
    // `x += (a, b),` parses as `x += ((a, b),)` — single-element tuple via
    // trailing comma. Symmetric with plain `x = (a, b),` which already works.
    let module = parse(
        "success_cases = []\n\
         success_cases += (1, 2),\n",
    );
    match &module.stmts[1].node {
        Stmt::AugAssign { value, .. } => match &value.node {
            Expr::TupleLit(elems) => {
                assert_eq!(
                    elems.len(),
                    1,
                    "expected single-element wrapper tuple, got {} elems",
                    elems.len()
                );
                assert!(
                    matches!(&elems[0].node, Expr::TupleLit(_)),
                    "expected inner (1, 2) tuple, got {:?}",
                    elems[0].node
                );
            }
            other => panic!("expected TupleLit RHS, got {other:?}"),
        },
        other => panic!("expected AugAssign, got {other:?}"),
    }
}
