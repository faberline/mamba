use super::*;

#[test]
pub(super) fn class_body_type_alias_reads_type_param_from_class_tuple() {
    let m = desugared("class Holder[T]:\n    type Inner = T\n");
    let body = find_class_body(&m.stmts, "Holder");
    let value = body
            .iter()
            .find_map(|stmt| match &stmt.node {
                Stmt::Assign { target, value }
                    if matches!(&target.node, Expr::Ident(name) if name == "Inner")
                        && matches!(
                            &value.node,
                            Expr::Call { func, args }
                                if matches!(&func.node, Expr::Ident(name) if name == TYPE_ALIAS_INTRINSIC)
                                    && matches!(
                                        args.get(1),
                                        Some(CallArg::Positional(arg))
                                            if matches!(&arg.node, Expr::Lambda { .. })
                                    )
                        ) =>
                {
                    Some(value)
                }
                _ => None,
            })
            .expect("real alias assignment");
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
        Expr::Index { object, index }
            if matches!(
                &object.node,
                Expr::Attr { object: cls, attr }
                    if attr == "__type_params__"
                        && matches!(&cls.node, Expr::Ident(name) if name == "Holder")
            )
                && matches!(&index.node, Expr::IntLit(0))
    ));
}
