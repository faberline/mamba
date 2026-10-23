use super::*;

#[test]
pub(super) fn test_param_annotation_presence_regular() {
    if let Stmt::FnDef { params, .. } = parse_stmt("def f(x):\n    pass\n") {
        assert!(params[0].is_omitted());
        assert!(!params[0].is_authored());
    } else {
        panic!("expected FnDef");
    }

    if let Stmt::FnDef { params, .. } = parse_stmt("def f(x: int):\n    pass\n") {
        assert!(params[0].is_authored());
        assert!(!params[0].is_omitted());
    } else {
        panic!("expected FnDef");
    }

    if let Stmt::FnDef { params, .. } = parse_stmt("def f(x: Any):\n    pass\n") {
        assert!(params[0].is_authored());
        assert!(!params[0].is_omitted());
    } else {
        panic!("expected FnDef");
    }
}
