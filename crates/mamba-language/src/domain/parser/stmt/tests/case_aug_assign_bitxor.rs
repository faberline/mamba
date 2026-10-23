use super::*;

#[test]
pub(super) fn test_aug_assign_bitxor() {
    match parse_stmt("x ^= 2\n") {
        Stmt::AugAssign { op, .. } => assert_eq!(op, AugOp::BitXor),
        other => panic!("expected AugAssign, got {other:?}"),
    }
}
