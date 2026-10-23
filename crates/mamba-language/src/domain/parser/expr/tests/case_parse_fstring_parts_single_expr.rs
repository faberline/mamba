use super::*;

#[test]
pub(super) fn test_parse_fstring_parts_single_expr() {
    let parts = parse_fstring_parts("hello {name}", false).unwrap();
    assert_eq!(parts.len(), 2);
    assert!(matches!(&parts[0], FStringPart::Literal(s) if s == "hello "));
    assert!(matches!(&parts[1], FStringPart::Expr(_, None)));
}
