use super::*;

// --- Class def ---

#[test]
pub(super) fn test_class_def_basic() {
    match parse_stmt("class Foo:\n    pass\n") {
        Stmt::ClassDef {
            name,
            bases,
            body,
            decorators,
            ..
        } => {
            assert_eq!(name, "Foo");
            assert!(bases.is_empty());
            assert!(decorators.is_empty());
            assert_eq!(body.len(), 1);
        }
        other => panic!("expected ClassDef, got {other:?}"),
    }
}
