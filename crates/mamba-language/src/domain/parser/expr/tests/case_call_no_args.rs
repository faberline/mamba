use super::*;

// --- Call ---

#[test]
pub(super) fn test_call_no_args() {
    match parse_expr_str("f()") {
        Expr::Call { func, args } => {
            assert!(matches!(func.node, Expr::Ident(ref n) if n == "f"));
            assert!(args.is_empty());
        }
        other => panic!("expected Call, got {other:?}"),
    }
}
