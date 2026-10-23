use super::*;

#[test]
pub(super) fn test_from_import() {
    match parse_stmt("from os import getcwd\n") {
        Stmt::Import { module, names, .. } => {
            assert_eq!(module, vec!["os"]);
            let names = names.unwrap();
            assert_eq!(names.len(), 1);
            assert_eq!(names[0].0, "getcwd");
            assert!(names[0].1.is_none());
        }
        other => panic!("expected Import, got {other:?}"),
    }
}
