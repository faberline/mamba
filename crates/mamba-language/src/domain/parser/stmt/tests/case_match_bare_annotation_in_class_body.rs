use super::*;

#[test]
pub(super) fn test_match_bare_annotation_in_class_body() {
    // #1923 — the original failing shape: a dataclass-style field named
    // `match` inside a class body.
    let src = "class Foo:\n    match: str\n    other: int\n";
    let module = parser::parse(src, fid()).expect("parse failed");
    match module.stmts.into_iter().next().unwrap().node {
        Stmt::ClassDef { body, .. } => {
            assert_eq!(body.len(), 2);
            match &body[0].node {
                Stmt::BareAnnotation { name, ty } => {
                    assert_eq!(name, "match");
                    assert!(matches!(ty.node, TypeExpr::Named(ref n) if n == "str"));
                }
                other => panic!("expected BareAnnotation, got {other:?}"),
            }
        }
        other => panic!("expected ClassDef, got {other:?}"),
    }
}
