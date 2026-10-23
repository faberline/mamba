use super::*;

#[test]
pub(super) fn test_try_except_as() {
    match parse_stmt("try:\n    pass\nexcept ValueError as e:\n    pass\n") {
        Stmt::Try { handlers, .. } => {
            assert!(handlers[0].exc_type.is_some());
            assert_eq!(handlers[0].name.as_deref(), Some("e"));
        }
        other => panic!("expected Try, got {other:?}"),
    }
}
