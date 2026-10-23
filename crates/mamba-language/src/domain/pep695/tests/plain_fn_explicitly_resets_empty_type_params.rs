use super::*;

#[test]
pub(super) fn plain_fn_explicitly_resets_empty_type_params() {
    let m = desugared("def f():\n    pass\n");
    assert_eq!(m.stmts.len(), 2);
    assert!(matches!(&m.stmts[0].node, Stmt::FnDef { .. }));
    match &m.stmts[1].node {
        Stmt::Assign { target, value } => {
            assert!(matches!(
                &target.node,
                Expr::Attr { attr, .. } if attr == "__type_params__"
            ));
            assert!(matches!(&value.node, Expr::TupleLit(items) if items.is_empty()));
        }
        other => panic!("expected __type_params__ reset assignment, got {other:?}"),
    }
}
