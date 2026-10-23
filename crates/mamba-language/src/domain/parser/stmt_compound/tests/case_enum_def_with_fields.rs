use super::*;

#[test]
pub(super) fn test_enum_def_with_fields() {
    match parse_stmt("enum Shape:\n    Circle(r: float)\n") {
        Stmt::EnumDef { variants, .. } => {
            assert_eq!(variants.len(), 1);
            assert_eq!(variants[0].name, "Circle");
            assert_eq!(variants[0].fields.len(), 1);
            assert_eq!(variants[0].fields[0].name, "r");
        }
        other => panic!("expected EnumDef, got {other:?}"),
    }
}
