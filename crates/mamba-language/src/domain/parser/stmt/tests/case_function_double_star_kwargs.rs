use super::*;

#[test]
pub(super) fn test_function_double_star_kwargs() {
    match parse_stmt("def foo(**kwargs: int):\n    pass\n") {
        Stmt::FnDef { params, .. } => {
            assert_eq!(params.len(), 1);
            assert_eq!(params[0].name, "kwargs");
            assert_eq!(params[0].kind, ParamKind::DoubleStar);
        }
        other => panic!("expected FnDef, got {other:?}"),
    }
}
