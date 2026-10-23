use super::*;

#[test]
pub(super) fn test_param_annotation_presence_double_star_kwargs() {
    if let Stmt::FnDef { params, .. } = parse_stmt("def f(**kwargs):\n    pass\n") {
        assert_eq!(params[0].kind, ParamKind::DoubleStar);
        assert!(params[0].is_omitted());
    } else {
        panic!("expected FnDef");
    }

    if let Stmt::FnDef { params, .. } = parse_stmt("def f(**kwargs: Any):\n    pass\n") {
        assert_eq!(params[0].kind, ParamKind::DoubleStar);
        assert!(params[0].is_authored());
    } else {
        panic!("expected FnDef");
    }
}
