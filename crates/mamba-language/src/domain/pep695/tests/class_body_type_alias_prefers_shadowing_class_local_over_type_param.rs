use super::*;

#[test]
pub(super) fn class_body_type_alias_prefers_shadowing_class_local_over_type_param() {
    let m = desugared("class C[T]:\n    T = \"class\"\n    type Alias = T\n");
    let Stmt::ClassDef { body, .. } = &m.stmts[1].node else {
        panic!("expected class definition after typevar binding");
    };
    let Stmt::Assign { target, value } = &body[3].node else {
        panic!("expected real alias assignment");
    };
    assert!(matches!(
        &target.node,
        Expr::Ident(name) if name == "Alias"
    ));
    let Expr::Call { args, .. } = &value.node else {
        panic!("expected type alias constructor");
    };
    let Some(CallArg::Positional(thunk)) = args.get(1) else {
        panic!("expected alias value thunk");
    };
    let Expr::Lambda { body, .. } = &thunk.node else {
        panic!("expected lambda thunk");
    };
    assert!(matches!(
        &body.node,
        Expr::Attr { object, attr }
            if attr == "T" && matches!(&object.node, Expr::Ident(name) if name == "C")
    ));
}
