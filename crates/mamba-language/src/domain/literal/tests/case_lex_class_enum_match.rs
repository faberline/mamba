use super::*;

#[test]
pub(super) fn test_lex_class_enum_match() {
    let kinds = lex_kinds("class enum match case");
    assert_eq!(
        kinds,
        vec![
            TokenKind::Class,
            TokenKind::Enum,
            TokenKind::Match,
            TokenKind::Case,
        ]
    );
}
