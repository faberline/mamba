use super::*;

#[test]
pub(super) fn test_match_bare_annotation_module_level() {
    // #1923 — `match: TYPE` with no subject tokens before the `:` is a
    // bare annotation on the identifier `match`, not a match-statement
    // (whose subject expression is grammatically mandatory).
    match parse_stmt("match: int\n") {
        Stmt::BareAnnotation { name, ty } => {
            assert_eq!(name, "match");
            assert!(matches!(ty.node, TypeExpr::Named(ref n) if n == "int"));
        }
        other => panic!("expected BareAnnotation, got {other:?}"),
    }
}
