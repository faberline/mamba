use super::*;

// --- Lambda *args / **kwargs ---

#[test]
pub(super) fn test_lambda_star_args() {
    match parse_expr("lambda *args: args") {
        Expr::Lambda { params, body } => {
            assert_eq!(params.len(), 1);
            assert_eq!(params[0].name, "args");
            assert_eq!(params[0].kind, ParamKind::Star);
            assert!(matches!(body.node, Expr::Ident(ref n) if n == "args"));
        }
        other => panic!("expected Lambda, got {other:?}"),
    }
}
