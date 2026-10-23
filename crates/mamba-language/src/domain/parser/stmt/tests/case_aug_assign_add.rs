use super::*;

// --- Augmented assignment ---

#[test]
pub(super) fn test_aug_assign_add() {
    match parse_stmt("x += 1\n") {
        Stmt::AugAssign { target, op, value } => {
            assert!(matches!(target.node, Expr::Ident(ref n) if n == "x"));
            assert_eq!(op, AugOp::Add);
            assert!(matches!(value.node, Expr::IntLit(1)));
        }
        other => panic!("expected AugAssign, got {other:?}"),
    }
}
