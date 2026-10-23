use super::*;

#[test]
pub(super) fn test_fn_def_with_type_params() {
    match parse_stmt("def foo[T, U]():\n    pass\n") {
        Stmt::FnDef { type_params, .. } => {
            let names: Vec<&str> = type_params.iter().map(|p| p.name.as_str()).collect();
            assert_eq!(names, vec!["T", "U"]);
        }
        other => panic!("expected FnDef, got {other:?}"),
    }
}
