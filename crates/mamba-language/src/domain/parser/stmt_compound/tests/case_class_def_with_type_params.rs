use super::*;

#[test]
pub(super) fn test_class_def_with_type_params() {
    match parse_stmt("class Foo[T]:\n    pass\n") {
        Stmt::ClassDef { type_params, .. } => {
            let names: Vec<&str> = type_params.iter().map(|p| p.name.as_str()).collect();
            assert_eq!(names, vec!["T"]);
        }
        other => panic!("expected ClassDef, got {other:?}"),
    }
}
