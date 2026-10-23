use super::*;

#[test]
pub(super) fn test_param_annotation_presence_pos_only() {
    if let Stmt::FnDef { params, .. } = parse_stmt("def f(x, /):\n    pass\n") {
        assert!(params[0].pos_only);
        assert!(params[0].is_omitted());
    } else {
        panic!("expected FnDef");
    }

    if let Stmt::FnDef { params, .. } = parse_stmt("def f(x: int, /):\n    pass\n") {
        assert!(params[0].pos_only);
        assert!(params[0].is_authored());
    } else {
        panic!("expected FnDef");
    }
}
