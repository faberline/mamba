use super::*;

#[test]
pub(super) fn test_aug_assign_mul() {
    match parse_stmt("x *= 2\n") {
        Stmt::AugAssign { op, .. } => assert_eq!(op, AugOp::Mul),
        other => panic!("expected AugAssign, got {other:?}"),
    }
}
