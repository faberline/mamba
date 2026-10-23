use super::*;

#[test]
pub(super) fn method_typevars_hoist_out_of_class_body() {
    let m = desugared("class C:\n    def meth[U](self):\n        return U\n");
    // Deferred wiring: class first, then U = typevar(...), then
    // `C.meth.__type_params__ = (U,)` (dotted path).
    assert_eq!(m.stmts.len(), 3);
    match &m.stmts[0].node {
        Stmt::ClassDef { body, .. } => {
            assert_eq!(body.len(), 1);
            assert!(matches!(&body[0].node, Stmt::FnDef { .. }));
        }
        other => panic!("expected ClassDef, got {other:?}"),
    }
    assert!(matches!(
        &m.stmts[1].node,
        Stmt::Assign { target, .. }
            if matches!(&target.node, Expr::Ident(n) if n == "U")
    ));
    match &m.stmts[2].node {
        Stmt::Assign { target, .. } => match &target.node {
            Expr::Attr { object, attr } => {
                assert_eq!(attr, "__type_params__");
                assert!(matches!(
                    &object.node,
                    Expr::Attr { object: cls, attr: meth }
                        if meth == "meth"
                            && matches!(&cls.node, Expr::Ident(n) if n == "C")
                ));
            }
            other => panic!("expected dotted Attr target, got {other:?}"),
        },
        other => panic!("expected Assign, got {other:?}"),
    }
}
