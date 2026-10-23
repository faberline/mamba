use super::*;

// --- Import ---

#[test]
pub(super) fn test_import_simple() {
    match parse_stmt("import os\n") {
        Stmt::Import {
            module,
            names,
            module_alias,
        } => {
            assert_eq!(module, vec!["os"]);
            assert!(names.is_none());
            assert!(module_alias.is_none());
        }
        other => panic!("expected Import, got {other:?}"),
    }
}
