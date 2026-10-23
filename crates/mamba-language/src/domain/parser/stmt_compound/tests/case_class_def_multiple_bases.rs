use super::*;

#[test]
pub(super) fn test_class_def_multiple_bases() {
    match parse_stmt("class Foo(Bar, Baz):\n    pass\n") {
        Stmt::ClassDef { bases, .. } => {
            assert_eq!(bases.len(), 2);
        }
        other => panic!("expected ClassDef, got {other:?}"),
    }
}
