use super::*;

#[test]
pub(super) fn test_match_tuple_subject() {
    let src = "match (1, 2):\n    case (x, y):\n        z = x\n";
    let module = parser::parse(src, fid()).expect("parse failed");
    match module.stmts.into_iter().next().unwrap().node {
        Stmt::Match { arms, .. } => {
            assert_eq!(arms.len(), 1);
            // The case pattern should be a Sequence pattern (x, y)
            assert!(matches!(&arms[0].pattern.node, Pattern::Sequence(_)));
        }
        other => panic!("expected Match, got {other:?}"),
    }
}
