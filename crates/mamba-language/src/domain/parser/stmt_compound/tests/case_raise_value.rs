use super::*;

#[test]
pub(super) fn test_raise_value() {
    match parse_stmt("raise ValueError()\n") {
        Stmt::Raise {
            value: Some(_),
            from: None,
        } => {}
        other => panic!("expected Raise(value), got {other:?}"),
    }
}
