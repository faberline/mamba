use super::*;

#[test]
pub(super) fn test_aug_assign_sub() {
    match parse_stmt("x -= 1\n") {
        Stmt::AugAssign { op, .. } => assert_eq!(op, AugOp::Sub),
        other => panic!("expected AugAssign, got {other:?}"),
    }
}
