use super::*;

#[test]
pub(super) fn test_type_alias_with_type_params() {
    match parse_stmt("type Container[T] = list[int]\n") {
        Stmt::TypeAlias {
            name, type_params, ..
        } => {
            assert_eq!(name, "Container");
            let names: Vec<&str> = type_params.iter().map(|p| p.name.as_str()).collect();
            assert_eq!(names, vec!["T"]);
        }
        other => panic!("expected TypeAlias, got {other:?}"),
    }
}
