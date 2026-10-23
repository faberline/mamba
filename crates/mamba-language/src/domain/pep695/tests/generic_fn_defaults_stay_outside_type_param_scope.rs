use super::*;

#[test]
pub(super) fn generic_fn_defaults_stay_outside_type_param_scope() {
    let m = desugared("def f[T](x = list[T]()):\n    return x\n");
    assert_eq!(m.stmts.len(), 3);
    assert!(matches!(&m.stmts[0].node, Stmt::FnDef { .. }));
    assert!(matches!(
        &m.stmts[1].node,
        Stmt::Assign { target, .. }
            if matches!(&target.node, Expr::Ident(n) if n == "T")
    ));
    assert!(matches!(
        &m.stmts[2].node,
        Stmt::Assign { target, .. }
            if matches!(&target.node, Expr::Attr { attr, .. } if attr == "__type_params__")
    ));
}
