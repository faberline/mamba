use super::*;

#[test]
pub(super) fn test_module_chained_assignment_desugaring_and_following_stmt() {
    let module = parse("a = b = c = 42\nx = 1\n");
    // 3 targets (a, b, c) desugar to 1 temp + 3 targets = 4 assignments, plus 1 for x = 1 -> total 5
    assert_eq!(module.stmts.len(), 5);

    // Stmt 0: __chained_<offset>__ = 42
    let tmp_name = match &module.stmts[0].node {
        Stmt::Assign { target, value } => {
            let name = match &target.node {
                Expr::Ident(n) => n.clone(),
                _ => panic!("expected Ident target for temp assign"),
            };
            assert!(
                name.starts_with("__chained_"),
                "expected temp name starting with __chained_"
            );
            assert!(
                matches!(&value.node, Expr::IntLit(42)),
                "temp should own original RHS once"
            );
            name
        }
        other => panic!("expected Stmt::Assign for temp, got {other:?}"),
    };

    // Stmt 1: a = __chained__
    match &module.stmts[1].node {
        Stmt::Assign { target, value } => {
            assert!(matches!(&target.node, Expr::Ident(n) if n == "a"));
            assert!(matches!(&value.node, Expr::Ident(n) if n == &tmp_name));
        }
        other => panic!("expected Stmt::Assign for target 'a', got {other:?}"),
    }

    // Stmt 2: b = __chained__
    match &module.stmts[2].node {
        Stmt::Assign { target, value } => {
            assert!(matches!(&target.node, Expr::Ident(n) if n == "b"));
            assert!(matches!(&value.node, Expr::Ident(n) if n == &tmp_name));
        }
        other => panic!("expected Stmt::Assign for target 'b', got {other:?}"),
    }

    // Stmt 3: c = __chained__
    match &module.stmts[3].node {
        Stmt::Assign { target, value } => {
            assert!(matches!(&target.node, Expr::Ident(n) if n == "c"));
            assert!(matches!(&value.node, Expr::Ident(n) if n == &tmp_name));
        }
        other => panic!("expected Stmt::Assign for target 'c', got {other:?}"),
    }

    // Stmt 4: x = 1 (parsed after all synthesized target assignments)
    match &module.stmts[4].node {
        Stmt::Assign { target, value } => {
            assert!(matches!(&target.node, Expr::Ident(n) if n == "x"));
            assert!(matches!(&value.node, Expr::IntLit(1)));
        }
        other => panic!("expected Stmt::Assign for following statement 'x = 1', got {other:?}"),
    }
}
