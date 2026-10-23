use super::*;

#[test]
pub(super) fn test_as_pattern_with_literal() {
    match parse_pattern("42 as n") {
        Pattern::As { pattern, name } => {
            assert_eq!(name, "n");
            assert!(matches!(&pattern.node, Pattern::Literal(Expr::IntLit(42))));
        }
        other => panic!("expected As, got {other:?}"),
    }
}
