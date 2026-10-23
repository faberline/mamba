use super::*;

#[test]
pub(super) fn test_try_except_typed() {
    match parse_stmt("try:\n    pass\nexcept ValueError:\n    pass\n") {
        Stmt::Try { handlers, .. } => {
            assert!(handlers[0].exc_type.is_some());
            assert!(handlers[0].name.is_none());
        }
        other => panic!("expected Try, got {other:?}"),
    }
}
