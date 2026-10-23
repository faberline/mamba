use super::*;

// --- Nested comprehension ---

#[test]
pub(super) fn test_nested_comprehension() {
    match parse_expr("[x for x in a for y in b]") {
        Expr::ListComp { generators, .. } => {
            assert_eq!(generators.len(), 2);
            assert_eq!(generators[0].targets, vec!["x"]);
            assert_eq!(generators[1].targets, vec!["y"]);
        }
        other => panic!("expected ListComp, got {other:?}"),
    }
}
