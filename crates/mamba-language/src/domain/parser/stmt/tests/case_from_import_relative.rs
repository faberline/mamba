use super::*;

#[test]
pub(super) fn test_from_import_relative() {
    match parse_stmt("from . import foo\n") {
        Stmt::Import { module, names, .. } => {
            assert_eq!(module[0], ".");
            assert!(names.is_some());
        }
        other => panic!("expected Import, got {other:?}"),
    }
}
