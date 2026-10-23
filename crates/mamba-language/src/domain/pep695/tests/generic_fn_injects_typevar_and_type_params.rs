use super::*;

#[test]
pub(super) fn generic_fn_injects_typevar_and_type_params() {
    let m = desugared("def f[T]():\n    return T\n");
    // def f, T = __mb_pep695_typevar__(...), f.__type_params__ = (T,)
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
