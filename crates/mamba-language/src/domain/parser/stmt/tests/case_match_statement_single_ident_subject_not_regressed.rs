use super::*;

#[test]
pub(super) fn test_match_statement_single_ident_subject_not_regressed() {
    // #1923 regression guard: a real match-statement with a single-token
    // (non-tuple) subject must still dispatch to parse_match(), since the
    // fix only changes behavior when zero subject tokens precede the `:`.
    let src = "match command:\n    case \"go\":\n        pass\n";
    let module = parser::parse(src, fid()).expect("parse failed");
    match module.stmts.into_iter().next().unwrap().node {
        Stmt::Match { .. } => {}
        other => panic!("expected Match, got {other:?}"),
    }
}
