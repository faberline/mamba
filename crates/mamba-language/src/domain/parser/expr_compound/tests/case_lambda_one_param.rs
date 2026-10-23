use super::*;

#[test]
pub(super) fn test_lambda_one_param() {
    match parse_expr("lambda x: x") {
        Expr::Lambda { params, body } => {
            assert_eq!(params.len(), 1);
            assert_eq!(params[0].name, "x");
            assert!(matches!(body.node, Expr::Ident(ref n) if n == "x"));
        }
        other => panic!("expected Lambda, got {other:?}"),
    }
}
