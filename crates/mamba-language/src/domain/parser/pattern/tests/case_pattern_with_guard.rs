use super::*;

// --- Guard ---

#[test]
pub(super) fn test_pattern_with_guard() {
    let src = "match x:\n    case y if y > 0:\n        pass\n";
    let module = parser::parse(src, fid()).expect("parse failed");
    match module.stmts.into_iter().next().unwrap().node {
        Stmt::Match { arms, .. } => {
            assert!(arms[0].guard.is_some());
            assert!(matches!(&arms[0].pattern.node, Pattern::Binding(n) if n == "y"));
        }
        other => panic!("expected Match, got {other:?}"),
    }
}
