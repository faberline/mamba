use super::*;

// #1590: parenthesized for-target `for (a, b) in ...` — outer parens
// are sugar around the tuple-target. Should parse identically to
// `for a, b in ...`.
#[test]
pub(super) fn test_for_paren_tuple_target() {
    let module = parse("for (a, b) in pairs:\n    pass\n");
    match &module.stmts[0].node {
        Stmt::For { targets, .. } => {
            assert_eq!(targets, &["a", "b"]);
        }
        _ => panic!("expected For"),
    }
}
