use super::*;

#[test]
pub(super) fn test_match_stmt() {
    let src = "match x:\n    case 1:\n        pass\n    case 2:\n        pass\n";
    let module = parse(src);
    assert_eq!(module.stmts.len(), 1);
    match &module.stmts[0].node {
        Stmt::Match { arms, .. } => {
            assert_eq!(arms.len(), 2);
        }
        _ => panic!("expected Match"),
    }
}
