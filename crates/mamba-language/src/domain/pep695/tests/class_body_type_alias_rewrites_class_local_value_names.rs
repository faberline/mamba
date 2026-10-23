use super::*;

#[test]
pub(super) fn class_body_type_alias_rewrites_class_local_value_names() {
    let m = desugared("class Holder:\n    member = int\n    type Inner = member\n");
    let Stmt::ClassDef { body, .. } = &m.stmts[0].node else {
        panic!("expected class definition");
    };
    let Stmt::Assign { value, .. } = &body[3].node else {
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
    assert!(matches!(
        &body.node,
        Expr::Attr { object, attr }
            if attr == "member"
                && matches!(&object.node, Expr::Ident(name) if name == "Holder")
    ));
}
