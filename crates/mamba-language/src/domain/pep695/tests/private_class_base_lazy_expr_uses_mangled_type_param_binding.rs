use super::*;

#[test]
pub(super) fn private_class_base_lazy_expr_uses_mangled_type_param_binding() {
    let m = desugared("class Foo[__T](make_base(lambda: __T)):\n    pass\n");
    let class_bases = m
        .stmts
        .iter()
        .find_map(|stmt| match &stmt.node {
            Stmt::ClassDef { name, bases, .. } if name == "Foo" => Some(bases),
            _ => None,
        })
        .expect("expected Foo class");
    let Expr::Call { args, .. } = &class_bases[0].node else {
        panic!("expected base constructor call");
    };
    let Some(CallArg::Positional(arg)) = args.first() else {
        panic!("expected lambda base argument");
    };
    let Expr::Lambda { body, .. } = &arg.node else {
        panic!("expected lazy base lambda");
    };
    assert!(matches!(&body.node, Expr::Ident(name) if name == "_Foo__T"));
    assert!(!m.stmts.iter().any(|stmt| {
        matches!(
            &stmt.node,
            Stmt::Del(expr) if matches!(&expr.node, Expr::Ident(name) if name == "_Foo__T")
        )
    }));
}
