use super::*;

#[test]
pub(super) fn generic_class_keeps_type_param_used_by_runtime_body() {
    let m = desugared("class C[T]:\n    def f(self):\n        return T\n");
    assert!(m.stmts.iter().any(|stmt| matches!(
        &stmt.node,
        Stmt::Assign { target, value }
            if matches!(&target.node, Expr::Attr { attr, .. } if attr == "__type_params__")
                && matches!(&value.node, Expr::TupleLit(items) if items.is_empty())
    )));
    assert!(!restores_or_deletes(&m.stmts, "T"));
}
