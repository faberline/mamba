use super::*;

#[test]
pub(super) fn test_class_def_metaclass_only() {
    // R6.1: class with ONLY a metaclass= keyword arg — bases list must be empty
    let src = "class Foo(metaclass=ABCMeta):\n    pass\n";
    let module = parse(src);
    match &module.stmts[0].node {
        Stmt::ClassDef { name, bases, .. } => {
            assert_eq!(name, "Foo");
            assert!(
                bases.is_empty(),
                "metaclass= should not appear in bases, got {bases:?}"
            );
        }
        other => panic!("expected ClassDef, got {other:?}"),
    }
}
