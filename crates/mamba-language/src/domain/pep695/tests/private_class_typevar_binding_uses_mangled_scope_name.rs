use super::*;

#[test]
pub(super) fn private_class_typevar_binding_uses_mangled_scope_name() {
    let m = desugared("class Foo[__T]:\n    param = __T\n");
    let (target, args) = find_typevar_assign(&m.stmts, "__T").expect("hoisted typevar assignment");
    assert!(matches!(&target.node, Expr::Ident(name) if name == "_Foo__T"));
    assert!(matches!(
        args.first(),
        Some(CallArg::Positional(arg)) if matches!(&arg.node, Expr::StrLit(name) if name == "__T")
    ));
    let value = m
            .stmts
            .iter()
            .find_map(|stmt| match &stmt.node {
                Stmt::Assign { target, value }
                    if matches!(&target.node, Expr::Attr { attr, .. } if attr == "__type_params__") =>
                {
                    Some(value)
                }
                _ => None,
            })
            .expect("__type_params__ assignment");
    let Expr::TupleLit(items) = &value.node else {
        panic!("expected params tuple");
    };
    assert!(matches!(
        items.first(),
        Some(item) if matches!(&item.node, Expr::Ident(name) if name == "_Foo__T")
    ));
}
