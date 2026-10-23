use super::*;

#[test]
pub(super) fn test_decorator_on_class() {
    match parse_stmt("@my_dec\nclass Foo:\n    pass\n") {
        Stmt::ClassDef {
            decorators, name, ..
        } => {
            assert_eq!(decorators.len(), 1);
            assert_eq!(name, "Foo");
        }
        other => panic!("expected ClassDef, got {other:?}"),
    }
}
