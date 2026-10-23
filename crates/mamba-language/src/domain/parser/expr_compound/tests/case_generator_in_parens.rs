use super::*;

// --- Generator expression ---

#[test]
pub(super) fn test_generator_in_parens() {
    match parse_expr("(x for x in items)") {
        Expr::GeneratorExpr {
            element,
            generators,
        } => {
            assert!(matches!(element.node, Expr::Ident(ref n) if n == "x"));
            assert_eq!(generators.len(), 1);
        }
        other => panic!("expected GeneratorExpr, got {other:?}"),
    }
}
