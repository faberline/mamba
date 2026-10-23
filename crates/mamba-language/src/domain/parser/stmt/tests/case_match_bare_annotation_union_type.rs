use super::*;

#[test]
pub(super) fn test_match_bare_annotation_union_type() {
    // #1923 — union-type annotation shouldn't change the disambiguation.
    match parse_stmt("match: str | None\n") {
        Stmt::BareAnnotation { name, ty } => {
            assert_eq!(name, "match");
            assert!(matches!(ty.node, TypeExpr::Union(ref elems) if elems.len() == 2));
        }
        other => panic!("expected BareAnnotation, got {other:?}"),
    }
}
