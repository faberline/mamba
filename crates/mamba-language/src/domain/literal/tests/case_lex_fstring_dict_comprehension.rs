use super::*;

#[test]
pub(super) fn test_lex_fstring_dict_comprehension() {
    // f"{ {k: v for k, v in items} }" — nested braces in expression
    let kinds = lex_kinds("f\"{ {k: v for k, v in items} }\"");
    assert_eq!(
        kinds,
        vec![TokenKind::FStr("{ {k: v for k, v in items} }".into())]
    );
}
