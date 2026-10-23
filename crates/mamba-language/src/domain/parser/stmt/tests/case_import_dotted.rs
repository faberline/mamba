use super::*;

#[test]
pub(super) fn test_import_dotted() {
    match parse_stmt("import os.path\n") {
        Stmt::Import { module, names, .. } => {
            assert_eq!(module, vec!["os", "path"]);
            assert!(names.is_none());
        }
        other => panic!("expected Import, got {other:?}"),
    }
}
