use super::*;

#[test]
pub(super) fn test_lambda_with_default() {
    match parse_expr("lambda x=1: x") {
        Expr::Lambda { params, .. } => {
            assert_eq!(params.len(), 1);
            assert!(params[0].default.is_some());
        }
        other => panic!("expected Lambda, got {other:?}"),
    }
}
