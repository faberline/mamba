use super::*;

/// Parse a match statement with a single arm and return the Pattern.
pub(super) fn parse_pattern(pat_src: &str) -> Pattern {
    let src = format!("match x:\n    case {pat_src}:\n        pass\n");
    let module = parser::parse(&src, fid()).expect("parse failed");
    match module.stmts.into_iter().next().unwrap().node {
        Stmt::Match { arms, .. } => arms.into_iter().next().unwrap().pattern.node,
        other => panic!("expected Match, got {other:?}"),
    }
}
