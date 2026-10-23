use super::*;

#[test]
pub(super) fn test_lambda_multiple_params() {
    match parse_expr("lambda a, b: a") {
        Expr::Lambda { params, .. } => {
            assert_eq!(params.len(), 2);
            assert_eq!(params[0].name, "a");
            assert_eq!(params[1].name, "b");
        }
        other => panic!("expected Lambda, got {other:?}"),
    }
}
