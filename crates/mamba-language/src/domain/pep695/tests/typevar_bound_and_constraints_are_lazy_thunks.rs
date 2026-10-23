use super::*;

#[test]
pub(super) fn typevar_bound_and_constraints_are_lazy_thunks() {
    let m = desugared("class C[T: Undefined, U: (Undefined,)]:\n    pass\n");
    let (target, args) = find_typevar_assign(&m.stmts, "T").expect("T typevar assignment");
    assert!(matches!(&target.node, Expr::Ident(n) if n == "T"));
    assert!(
        matches!(args.get(2), Some(CallArg::Positional(arg)) if matches!(arg.node, Expr::Lambda { .. }))
    );
    assert!(
        matches!(args.get(3), Some(CallArg::Positional(arg)) if matches!(arg.node, Expr::NoneLit))
    );
    assert!(
        matches!(args.get(4), Some(CallArg::Positional(arg)) if matches!(arg.node, Expr::NoneLit))
    );

    let (target, args) = find_typevar_assign(&m.stmts, "U").expect("U typevar assignment");
    assert!(matches!(&target.node, Expr::Ident(n) if n == "U"));
    assert!(
        matches!(args.get(2), Some(CallArg::Positional(arg)) if matches!(arg.node, Expr::NoneLit))
    );
    assert!(
        matches!(args.get(3), Some(CallArg::Positional(arg)) if matches!(arg.node, Expr::Lambda { .. }))
    );
    assert!(
        matches!(args.get(4), Some(CallArg::Positional(arg)) if matches!(arg.node, Expr::NoneLit))
    );
}
