use super::*;

#[test]
pub(super) fn test_chained_tuple_unpack_assign() {
    // `ka, va = ta = expr` — chain a tuple-unpack target with a simple target.
    // Both share the same RHS via a `__chained_N__` temp; mainline emits the
    // temp-assign stmt and queues the per-target assignments via pending_stmts.
    let module = parse(
        "def f():\n\
         \x20   ka, va = ta = (1, 2)\n",
    );
    match &module.stmts[0].node {
        Stmt::FnDef { body, .. } => {
            // Expect 3 stmts in the desugared body: __chained = (1,2);
            // (ka, va) = __chained; ta = __chained.
            assert_eq!(
                body.len(),
                3,
                "expected 3 desugared stmts, got {}: {:?}",
                body.len(),
                body
            );
            // First stmt: __chained_N__ = (1, 2)
            match &body[0].node {
                Stmt::Assign { target, .. } => {
                    if let Expr::Ident(n) = &target.node {
                        assert!(
                            n.starts_with("__chained_"),
                            "expected chained temp, got {n}"
                        );
                    } else {
                        panic!("expected ident target on temp assign, got {target:?}");
                    }
                }
                other => panic!("expected Assign, got {other:?}"),
            }
            // Second stmt: (ka, va) = __chained
            match &body[1].node {
                Stmt::Assign { target, .. } => {
                    assert!(
                        matches!(&target.node, Expr::TupleLit(_)),
                        "expected TupleLit target, got {target:?}"
                    );
                }
                other => panic!("expected Assign, got {other:?}"),
            }
            // Third stmt: ta = __chained
            match &body[2].node {
                Stmt::Assign { target, .. } => {
                    assert!(
                        matches!(&target.node, Expr::Ident(_)),
                        "expected Ident target, got {target:?}"
                    );
                }
                other => panic!("expected Assign, got {other:?}"),
            }
        }
        other => panic!("expected FnDef, got {other:?}"),
    }
}
