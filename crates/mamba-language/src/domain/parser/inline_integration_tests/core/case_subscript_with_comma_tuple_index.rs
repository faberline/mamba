use super::*;

#[test]
pub(super) fn test_subscript_with_comma_tuple_index() {
    // `m[0, 1]` is shorthand for `m[(0, 1)]` — the comma builds an implicit
    // tuple inside the subscript. Common with dict-with-tuple-key access.
    let module = parse(
        "m = {}\n\
         x = m[0, 1]\n",
    );
    match &module.stmts[1].node {
        Stmt::Assign { value, .. } => match &value.node {
            Expr::Index { index, .. } => match &index.node {
                Expr::TupleLit(elems) => {
                    assert_eq!(
                        elems.len(),
                        2,
                        "expected 2-element tuple index, got {} elems",
                        elems.len()
                    );
                }
                other => panic!("expected TupleLit subscript index, got {other:?}"),
            },
            other => panic!("expected Index expr, got {other:?}"),
        },
        other => panic!("expected Assign, got {other:?}"),
    }
}
