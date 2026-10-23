use super::*;

#[test]
pub(super) fn test_import() {
    let module = parse("import math\n");
    match &module.stmts[0].node {
        Stmt::Import {
            module: m, names, ..
        } => {
            assert_eq!(m, &["math"]);
            assert!(names.is_none());
        }
        _ => panic!("expected Import"),
    }
}
