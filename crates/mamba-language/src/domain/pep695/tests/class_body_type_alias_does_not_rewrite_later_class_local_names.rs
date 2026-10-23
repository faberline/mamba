use super::*;

#[test]
pub(super) fn class_body_type_alias_does_not_rewrite_later_class_local_names() {
    let m = desugared("class C:\n    type Alias = x\n    x = \"class\"\n");
    let Stmt::ClassDef { body, .. } = &m.stmts[0].node else {
        panic!("expected class definition");
    };
    let Stmt::Assign { value, .. } = &body[2].node else {
        panic!("expected real alias assignment");
    };
    let Expr::Call { args, .. } = &value.node else {
        panic!("expected type alias constructor");
    };
    let Some(CallArg::Positional(thunk)) = args.get(1) else {
        panic!("expected alias value thunk");
    };
    let Expr::Lambda { body, .. } = &thunk.node else {
        panic!("expected lambda thunk");
    };
    assert!(matches!(&body.node, Expr::Ident(name) if name == "x"));
}
