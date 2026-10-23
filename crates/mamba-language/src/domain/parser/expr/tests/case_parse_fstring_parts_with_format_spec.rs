use super::*;

#[test]
pub(super) fn test_parse_fstring_parts_with_format_spec() {
    let parts = parse_fstring_parts("{x:.2f}", false).unwrap();
    assert_eq!(parts.len(), 1);
    match &parts[0] {
        FStringPart::Expr(_, Some(spec)) => {
            assert!(matches!(&spec[..], [FStringPart::Literal(l)] if l == ".2f"));
        }
        other => panic!("expected Expr with spec, got {other:?}"),
    }
}
