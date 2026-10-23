use super::*;

#[test]
pub(super) fn test_from_import_alias() {
    match parse_stmt("from os import getcwd as gw\n") {
        Stmt::Import { names, .. } => {
            let names = names.unwrap();
            assert_eq!(names[0].0, "getcwd");
            assert_eq!(names[0].1.as_deref(), Some("gw"));
        }
        other => panic!("expected Import, got {other:?}"),
    }
}
