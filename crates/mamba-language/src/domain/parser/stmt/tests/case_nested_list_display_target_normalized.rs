use super::*;

#[test]
pub(super) fn test_nested_list_display_target_normalized() {
    // `[a, [b, c]] = xs` — nested list display normalizes recursively.
    match parse_stmt("[a, [b, c]] = xs\n") {
        Stmt::Assign { target, .. } => match target.node {
            Expr::TupleLit(elems) => {
                assert_eq!(elems.len(), 2);
                assert!(matches!(&elems[1].node, Expr::TupleLit(inner) if inner.len() == 2));
            }
            other => panic!("expected TupleLit target, got {other:?}"),
        },
        other => panic!("expected Assign, got {other:?}"),
    }
}
