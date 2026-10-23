use super::*;

// --- List comprehension ---

#[test]
pub(super) fn test_list_comp() {
    match parse_expr("[x for x in items]") {
        Expr::ListComp {
            element,
            generators,
        } => {
            assert!(matches!(element.node, Expr::Ident(ref n) if n == "x"));
            assert_eq!(generators.len(), 1);
            assert_eq!(generators[0].targets, vec!["x"]);
            assert!(generators[0].conditions.is_empty());
        }
        other => panic!("expected ListComp, got {other:?}"),
    }
}
