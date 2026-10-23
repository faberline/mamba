use super::*;

#[test]
pub(super) fn type_alias_keeps_stmt_and_adds_binding() {
    let m = desugared("type X = int\n");
    // TypeAlias + placeholder binding + real binding.
    assert_eq!(m.stmts.len(), 3);
    assert!(matches!(&m.stmts[0].node, Stmt::TypeAlias { .. }));
    for s in &m.stmts[1..] {
        assert!(matches!(
            &s.node,
            Stmt::Assign { target, .. }
                if matches!(&target.node, Expr::Ident(n) if n == "X")
        ));
    }
}
