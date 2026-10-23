use super::*;

#[test]
pub(super) fn test_lambda_double_star_kwargs() {
    match parse_expr("lambda **kwargs: kwargs") {
        Expr::Lambda { params, body } => {
            assert_eq!(params.len(), 1);
            assert_eq!(params[0].name, "kwargs");
            assert_eq!(params[0].kind, ParamKind::DoubleStar);
            assert!(matches!(body.node, Expr::Ident(ref n) if n == "kwargs"));
        }
        other => panic!("expected Lambda, got {other:?}"),
    }
}
