use super::*;

#[test]
pub(super) fn test_or_pattern() {
    // Literal alternatives: capture patterns in non-final alternation
    // position are a compile-time SyntaxError (CPython pattern rules),
    // so `a | b | c` is no longer parseable inside a case.
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
