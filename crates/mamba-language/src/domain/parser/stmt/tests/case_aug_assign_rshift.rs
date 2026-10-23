use super::*;

#[test]
pub(super) fn test_aug_assign_rshift() {
    match parse_stmt("x >>= 2\n") {
        Stmt::AugAssign { op, .. } => assert_eq!(op, AugOp::RShift),
        other => panic!("expected AugAssign, got {other:?}"),
    }
}
