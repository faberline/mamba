use super::*;

#[test]
pub(super) fn test_nonlocal() {
    match parse_stmt("nonlocal x\n") {
        Stmt::Nonlocal(names) => {
            assert_eq!(names, vec!["x"]);
        }
        other => panic!("expected Nonlocal, got {other:?}"),
    }
}
