use super::*;

#[test]
pub(super) fn test_multiple_decorators() {
    match parse_stmt("@dec1\n@dec2\ndef foo():\n    pass\n") {
        Stmt::FnDef { decorators, .. } => {
            assert_eq!(decorators.len(), 2);
        }
        other => panic!("expected FnDef, got {other:?}"),
    }
}
