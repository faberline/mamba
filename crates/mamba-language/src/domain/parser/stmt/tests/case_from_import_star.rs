use super::*;

#[test]
pub(super) fn test_from_import_star() {
    match parse_stmt("from os import *\n") {
        Stmt::Import { names, .. } => {
            let names = names.unwrap();
            assert_eq!(names[0].0, "*");
        }
        other => panic!("expected Import(*), got {other:?}"),
    }
}
