use super::*;

#[test]
pub(super) fn test_aug_assign_bitor() {
    match parse_stmt("x |= 2\n") {
        Stmt::AugAssign { op, .. } => assert_eq!(op, AugOp::BitOr),
        other => panic!("expected AugAssign, got {other:?}"),
    }
}
