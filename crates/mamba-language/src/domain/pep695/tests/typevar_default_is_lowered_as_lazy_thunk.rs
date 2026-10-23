use super::*;

#[test]
pub(super) fn typevar_default_is_lowered_as_lazy_thunk() {
    let m = desugared("def f[T = Undefined]():\n    return T\n");
    let (_, args) = find_typevar_assign(&m.stmts, "T").expect("T typevar assignment");
    assert!(matches!(
        args.get(4),
        Some(CallArg::Positional(arg)) if matches!(arg.node, Expr::Lambda { .. })
    ));
}
