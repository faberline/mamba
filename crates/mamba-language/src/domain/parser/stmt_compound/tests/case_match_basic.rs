use super::*;

// --- Match (basic, tested more in pattern.rs) ---

#[test]
pub(super) fn test_match_basic() {
    match parse_stmt("match x:\n    case 1:\n        pass\n") {
        Stmt::Match { expr, arms } => {
            assert!(matches!(expr.node, Expr::Ident(ref n) if n == "x"));
            assert_eq!(arms.len(), 1);
        }
        other => panic!("expected Match, got {other:?}"),
    }
}
