use super::*;

#[test]
pub(super) fn private_method_and_alias_typevar_bindings_use_mangled_scope_names() {
    let m = desugared(
        "class Foo:\n    def meth[__U](self):\n        return __U\n    type Alias[__V] = __V\n",
    );
    let (target, args) = find_typevar_assign(&m.stmts, "__V").expect("alias typevar assignment");
    assert!(matches!(&target.node, Expr::Ident(name) if name == "_Foo__V"));
    assert!(matches!(
        args.first(),
        Some(CallArg::Positional(arg))
            if matches!(&arg.node, Expr::StrLit(name) if name == "__V")
    ));
    let (target, args) = find_typevar_assign(&m.stmts, "__U").expect("method typevar assignment");
    assert!(matches!(&target.node, Expr::Ident(name) if name == "_Foo__U"));
    assert!(matches!(
        args.first(),
        Some(CallArg::Positional(arg))
            if matches!(&arg.node, Expr::StrLit(name) if name == "__U")
    ));

    let class_body = find_class_body(&m.stmts, "Foo");
    let (target, value) = class_body
            .iter()
            .find_map(|stmt| match &stmt.node {
                Stmt::Assign { target, value }
                    if matches!(&target.node, Expr::Ident(name) if name == "Alias")
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
                    Some((target, value))
                }
                _ => None,
            })
            .expect("real alias assignment");
    assert!(matches!(&target.node, Expr::Ident(name) if name == "Alias"));
    let Expr::Call { args, .. } = &value.node else {
        panic!("expected type alias constructor");
    };
    assert!(matches!(
        args.get(2),
        Some(CallArg::Positional(arg))
            if matches!(
                &arg.node,
                Expr::TupleLit(items)
                    if matches!(
                        items.first(),
                        Some(item)
                            if matches!(&item.node, Expr::Ident(name) if name == "_Foo__V")
                    )
            )
    ));
    let value = m
        .stmts
        .iter()
        .find_map(|stmt| match &stmt.node {
            Stmt::Assign { target, value }
                if matches!(
                    &target.node,
                    Expr::Attr { object, attr }
                        if attr == "__type_params__"
                            && matches!(
                                &object.node,
                                Expr::Attr { object: cls, attr: method }
                                    if method == "meth"
                                        && matches!(&cls.node, Expr::Ident(name) if name == "Foo")
                            )
                ) =>
            {
                Some(value)
            }
            _ => None,
        })
        .expect("method __type_params__ assignment");
    let Expr::TupleLit(items) = &value.node else {
        panic!("expected method params tuple");
    };
    assert!(matches!(
        items.first(),
        Some(item) if matches!(&item.node, Expr::Ident(name) if name == "_Foo__U")
    ));
}
