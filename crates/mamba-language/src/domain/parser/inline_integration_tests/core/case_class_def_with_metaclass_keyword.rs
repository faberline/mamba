use super::*;

// ── R6: metaclass= keyword in class declaration ──

#[test]
pub(super) fn test_class_def_with_metaclass_keyword() {
    // R6.1: class Foo(object, metaclass=Meta) must parse without error.
    // metaclass= is a keyword arg: it should NOT appear in the bases list.
    let src = "class Meta(type):\n    pass\nclass Foo(object, metaclass=Meta):\n    pass\n";
    let module = parse(src);
    assert_eq!(module.stmts.len(), 2);
    match &module.stmts[1].node {
        Stmt::ClassDef { name, bases, .. } => {
            assert_eq!(name, "Foo");
            // Only positional base 'object' should appear; metaclass= must be filtered out
            assert_eq!(
                bases.len(),
                1,
                "metaclass= keyword arg must not appear in bases, got {bases:?}"
            );
            assert!(matches!(&bases[0].node, Expr::Ident(n) if n == "object"));
        }
        other => panic!("expected ClassDef for Foo, got {other:?}"),
    }
}
