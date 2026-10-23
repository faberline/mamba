use super::*;

#[test]
pub(super) fn test_aug_assign_mod() {
    match parse_stmt("x %= 2\n") {
        Stmt::AugAssign { op, .. } => assert_eq!(op, AugOp::Mod),
        other => panic!("expected AugAssign, got {other:?}"),
    }
}
