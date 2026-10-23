use super::*;

#[test]
pub(super) fn test_class_def_with_base() {
    match parse_stmt("class Foo(Bar):\n    pass\n") {
        Stmt::ClassDef { bases, .. } => {
            assert_eq!(bases.len(), 1);
        }
        other => panic!("expected ClassDef, got {other:?}"),
    }
}
