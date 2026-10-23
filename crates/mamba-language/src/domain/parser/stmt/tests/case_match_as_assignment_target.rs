use super::*;

// --- Soft keyword disambiguation ---

#[test]
pub(super) fn test_match_as_assignment_target() {
    match parse_stmt("match = 1\n") {
        Stmt::Assign { target, .. } => {
            assert!(matches!(target.node, Expr::Ident(ref n) if n == "match"));
        }
        other => panic!("expected Assign, got {other:?}"),
    }
}
