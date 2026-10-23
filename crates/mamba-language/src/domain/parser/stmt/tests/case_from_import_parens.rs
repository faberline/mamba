use super::*;

#[test]
pub(super) fn test_from_import_parens() {
    match parse_stmt("from os import (getcwd, listdir)\n") {
        Stmt::Import { names, .. } => {
            let names = names.unwrap();
            assert_eq!(names.len(), 2);
            assert_eq!(names[0].0, "getcwd");
            assert_eq!(names[1].0, "listdir");
        }
        other => panic!("expected Import, got {other:?}"),
    }
}
