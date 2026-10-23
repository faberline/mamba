use super::*;

#[test]
pub(super) fn test_param_annotation_presence_star_args() {
    if let Stmt::FnDef { params, .. } = parse_stmt("def f(*args):\n    pass\n") {
        assert_eq!(params[0].kind, ParamKind::Star);
        assert!(params[0].is_omitted());
    } else {
        panic!("expected FnDef");
    }

    if let Stmt::FnDef { params, .. } = parse_stmt("def f(*args: Any):\n    pass\n") {
        assert_eq!(params[0].kind, ParamKind::Star);
        assert!(params[0].is_authored());
    } else {
        panic!("expected FnDef");
    }
}
