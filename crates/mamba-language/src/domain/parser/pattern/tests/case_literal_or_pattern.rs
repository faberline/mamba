use super::*;

// --- OR pattern ---

#[test]
pub(super) fn test_literal_or_pattern() {
    match parse_pattern("1 | 2 | 3") {
        Pattern::Or(alts) => {
            assert_eq!(alts.len(), 3);
            assert!(matches!(&alts[0].node, Pattern::Literal(Expr::IntLit(1))));
            assert!(matches!(&alts[1].node, Pattern::Literal(Expr::IntLit(2))));
            assert!(matches!(&alts[2].node, Pattern::Literal(Expr::IntLit(3))));
        }
        other => panic!("expected Or, got {other:?}"),
    }
}
