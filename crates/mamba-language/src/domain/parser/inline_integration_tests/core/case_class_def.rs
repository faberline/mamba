use super::*;

#[test]
pub(super) fn test_class_def() {
    let src = "class Point:\n    x: int = 0\n    y: int = 0\n";
    let module = parse(src);
    assert_eq!(module.stmts.len(), 1);
    match &module.stmts[0].node {
        Stmt::ClassDef { name, body, .. } => {
            assert_eq!(name, "Point");
            assert_eq!(body.len(), 2);
        }
        _ => panic!("expected ClassDef"),
    }
}
