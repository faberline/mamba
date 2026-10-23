use super::*;

// --- Type keywords ---

#[test]
pub(super) fn test_lex_type_keywords() {
    let kinds = lex_kinds("int float bool str list dict tuple");
    assert_eq!(
        kinds,
        vec![
            TokenKind::IntType,
            TokenKind::FloatType,
            TokenKind::BoolType,
            TokenKind::StrType,
            TokenKind::ListType,
            TokenKind::DictType,
            TokenKind::TupleType,
        ]
    );
}
