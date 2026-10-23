use super::*;

// --- Enum def ---

#[test]
pub(super) fn test_enum_def_basic() {
    match parse_stmt("enum Color:\n    Red\n    Green\n    Blue\n") {
        Stmt::EnumDef { name, variants, .. } => {
            assert_eq!(name, "Color");
            assert_eq!(variants.len(), 3);
            assert_eq!(variants[0].name, "Red");
            assert_eq!(variants[1].name, "Green");
            assert_eq!(variants[2].name, "Blue");
        }
        other => panic!("expected EnumDef, got {other:?}"),
    }
}
