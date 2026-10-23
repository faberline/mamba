use super::*;

#[test]
pub(super) fn pep695_method_bound_rewrites_class_scope_names() {
    let m = desugared(
            "class X[A]:\n    T = int\n    cls = 'class'\n    def foo[U: T, V: cls, W: A](self):\n        pass\n",
        );
    let mut assigns = m.stmts.iter().filter_map(|stmt| match &stmt.node {
        Stmt::Assign { target, value } => match &target.node {
            Expr::Ident(name) if matches!(name.as_str(), "U" | "V" | "W") => {
                Some((name.as_str(), value))
            }
            _ => None,
        },
        _ => None,
    });

    fn expect_bound_body(value: &Spanned<Expr>) -> &Expr {
        let Expr::Call { args, .. } = &value.node else {
            panic!("expected typevar constructor");
        };
        let Some(CallArg::Positional(bound)) = args.get(2) else {
            panic!("expected bound thunk");
        };
        let Expr::Lambda { body, .. } = &bound.node else {
            panic!("expected bound lambda");
        };
        &body.node
    }

    let u_bound = expect_bound_body(assigns.find(|(name, _)| *name == "U").unwrap().1);
    assert!(matches!(
        u_bound,
        Expr::Attr { object, attr }
            if attr == "T" && matches!(&object.node, Expr::Ident(name) if name == "X")
    ));

    let v_bound = expect_bound_body(assigns.find(|(name, _)| *name == "V").unwrap().1);
    assert!(matches!(
        v_bound,
        Expr::Attr { object, attr }
            if attr == "cls" && matches!(&object.node, Expr::Ident(name) if name == "X")
    ));

    let w_bound = expect_bound_body(assigns.find(|(name, _)| *name == "W").unwrap().1);
    assert!(matches!(
        w_bound,
        Expr::Index { object, index }
            if matches!(
                &object.node,
                Expr::Attr { object: cls, attr }
                    if attr == "__type_params__"
                        && matches!(&cls.node, Expr::Ident(name) if name == "X")
            ) && matches!(&index.node, Expr::IntLit(0))
    ));
}
