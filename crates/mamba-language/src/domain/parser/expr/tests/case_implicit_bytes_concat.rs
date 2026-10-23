use super::*;

#[test]
pub(super) fn test_implicit_bytes_concat() {
    match parse_expr_str("b\"foo\" b\"bar\"") {
        Expr::BytesLit(b) => assert_eq!(b, b"foobar"),
        other => panic!("expected BytesLit, got {other:?}"),
    }
}
