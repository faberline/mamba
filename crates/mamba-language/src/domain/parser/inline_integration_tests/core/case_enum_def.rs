use super::*;

#[test]
pub(super) fn test_enum_def() {
    let src = "enum Shape:\n    Circle(radius: float)\n    Point\n";
    let module = parse(src);
    assert_eq!(module.stmts.len(), 1);
    match &module.stmts[0].node {
        Stmt::EnumDef { name, variants, .. } => {
            assert_eq!(name, "Shape");
            assert_eq!(variants.len(), 2);
            assert_eq!(variants[0].name, "Circle");
            assert_eq!(variants[0].fields.len(), 1);
            assert_eq!(variants[1].name, "Point");
            assert!(variants[1].fields.is_empty());
        }
        _ => panic!("expected EnumDef"),
    }
}
