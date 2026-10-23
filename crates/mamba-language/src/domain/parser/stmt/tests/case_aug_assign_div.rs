use super::*;

#[test]
pub(super) fn test_aug_assign_div() {
    match parse_stmt("x /= 2\n") {
        Stmt::AugAssign { op, .. } => assert_eq!(op, AugOp::Div),
        other => panic!("expected AugAssign, got {other:?}"),
    }
}
