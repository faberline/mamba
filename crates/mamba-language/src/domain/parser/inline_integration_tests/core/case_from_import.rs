use super::*;

#[test]
pub(super) fn test_from_import() {
    let module = parse("from math import sqrt, pi\n");
    match &module.stmts[0].node {
        Stmt::Import {
            module: m, names, ..
        } => {
            assert_eq!(m, &["math"]);
            let names = names.as_ref().unwrap();
            assert_eq!(names.len(), 2);
            assert_eq!(names[0].0, "sqrt");
            assert_eq!(names[1].0, "pi");
        }
        _ => panic!("expected Import"),
    }
}
