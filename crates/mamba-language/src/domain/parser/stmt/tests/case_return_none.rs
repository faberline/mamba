use super::*;

// --- Return ---

#[test]
pub(super) fn test_return_none() {
    match parse_stmt("return\n") {
        Stmt::Return(None) => {}
        other => panic!("expected Return(None), got {other:?}"),
    }
}
