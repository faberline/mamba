use super::*;

#[test]
pub(super) fn test_single_line_suite_chained_assignment_with_semicolon() {
    let module = parse("if flag: a = b = 1; x = 2\n");
    assert_eq!(module.stmts.len(), 1);
    match &module.stmts[0].node {
        Stmt::If { body, .. } => {
            assert_eq!(body.len(), 4);

            // body[0]: __chained_<offset>__ = 1
            let tmp_name = match &body[0].node {
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
                        matches!(&value.node, Expr::IntLit(1)),
                        "temp should own original RHS IntLit(1)"
                    );
                    name
                }
                other => panic!("expected Stmt::Assign for temp, got {other:?}"),
            };

            // body[1]: a = __chained__
            match &body[1].node {
                Stmt::Assign { target, value } => {
                    assert!(matches!(&target.node, Expr::Ident(n) if n == "a"));
                    assert!(matches!(&value.node, Expr::Ident(n) if n == &tmp_name));
                }
                other => panic!("expected Stmt::Assign for target 'a', got {other:?}"),
            }

            // body[2]: b = __chained__
            match &body[2].node {
                Stmt::Assign { target, value } => {
                    assert!(matches!(&target.node, Expr::Ident(n) if n == "b"));
                    assert!(matches!(&value.node, Expr::Ident(n) if n == &tmp_name));
                }
                other => panic!("expected Stmt::Assign for target 'b', got {other:?}"),
            }

            // body[3]: x = 2
            match &body[3].node {
                Stmt::Assign { target, value } => {
                    assert!(matches!(&target.node, Expr::Ident(n) if n == "x"));
                    assert!(matches!(&value.node, Expr::IntLit(2)));
                }
                other => panic!("expected Stmt::Assign for 'x', got {other:?}"),
            }
        }
        other => panic!("expected Stmt::If, got {other:?}"),
    }
}
