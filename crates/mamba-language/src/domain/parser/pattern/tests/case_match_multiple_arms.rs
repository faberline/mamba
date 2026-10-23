use super::*;

// --- Multiple arms ---

#[test]
pub(super) fn test_match_multiple_arms() {
    let src = "match x:\n    case 1:\n        pass\n    case _:\n        pass\n";
    let module = parser::parse(src, fid()).expect("parse failed");
    match module.stmts.into_iter().next().unwrap().node {
        Stmt::Match { arms, .. } => {
            assert_eq!(arms.len(), 2);
            assert!(matches!(&arms[1].pattern.node, Pattern::Wildcard));
        }
        other => panic!("expected Match, got {other:?}"),
    }
}
