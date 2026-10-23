use super::*;

#[test]
pub(super) fn test_subscript_with_three_comma_indices() {
    // `arr[a, b, c]` — generalize past 2-element case.
    let module = parse(
        "m = {}\n\
         x = m[1, 2, 3]\n",
    );
    match &module.stmts[1].node {
        Stmt::Assign { value, .. } => match &value.node {
            Expr::Index { index, .. } => match &index.node {
                Expr::TupleLit(elems) => assert_eq!(elems.len(), 3),
                other => panic!("expected TupleLit, got {other:?}"),
            },
            other => panic!("expected Index, got {other:?}"),
        },
        other => panic!("expected Assign, got {other:?}"),
    }
}
