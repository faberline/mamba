use super::*;

#[test]
pub(super) fn test_param_annotation_presence_self_and_method() {
    if let Stmt::FnDef { params, .. } = parse_stmt("def m(self):\n    pass\n") {
        assert_eq!(params[0].name, "self");
        assert!(params[0].is_omitted());
    } else {
        panic!("expected FnDef");
    }

    if let Stmt::FnDef { params, .. } = parse_stmt("def m(self: Self):\n    pass\n") {
        assert_eq!(params[0].name, "self");
        assert!(params[0].is_authored());
    } else {
        panic!("expected FnDef");
    }
}
