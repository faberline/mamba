use super::*;

#[test]
pub(super) fn test_param_annotation_presence_lambda() {
    match parse_expr("lambda x: x") {
        Expr::Lambda { params, .. } => {
            assert!(params[0].is_omitted());
            assert!(!params[0].is_authored());
        }
        other => panic!("expected Lambda, got {other:?}"),
    }

    match parse_expr("lambda x: int: x") {
        Expr::Lambda { params, .. } => {
            assert!(params[0].is_authored());
            assert!(!params[0].is_omitted());
        }
        other => panic!("expected Lambda, got {other:?}"),
    }
}
