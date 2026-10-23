use super::*;

#[test]
pub(super) fn test_lex_triple_with_escape() {
    let kinds = lex_kinds("\"\"\"has \\\"\"\" inside\"\"\"");
    assert_eq!(
        kinds,
        vec![TokenKind::TripleStr("has \"\"\" inside".into())]
    );
}
