use super::*;

#[test]
pub(super) fn test_import_alias() {
    match parse_stmt("import sys as system\n") {
        Stmt::Import {
            module,
            names,
            module_alias,
        } => {
            assert_eq!(module, vec!["sys"]);
            assert!(names.is_none());
            assert_eq!(module_alias.as_deref(), Some("system"));
        }
        other => panic!("expected Import, got {other:?}"),
    }
}
