use super::*;

#[test]
pub(super) fn pep695_method_bound_does_not_rewrite_later_class_local_names() {
    let m = desugared("class C:\n    def foo[T: x](self):\n        pass\n    x = \"class\"\n");
    let Stmt::Assign { value, .. } = &m.stmts[1].node else {
        panic!("expected hoisted typevar assignment");
    };
    let Expr::Call { args, .. } = &value.node else {
        panic!("expected typevar constructor");
    };
    let Some(CallArg::Positional(bound)) = args.get(2) else {
        panic!("expected bound thunk");
    };
    let Expr::Lambda { body, .. } = &bound.node else {
        panic!("expected bound lambda");
    };
    assert!(matches!(&body.node, Expr::Ident(name) if name == "x"));
}
