use super::*;

// --- Lambda ---

#[test]
pub(super) fn test_lambda_no_params() {
    match parse_expr("lambda: 42") {
        Expr::Lambda { params, body } => {
            assert!(params.is_empty());
            assert!(matches!(body.node, Expr::IntLit(42)));
        }
        other => panic!("expected Lambda, got {other:?}"),
    }
}
