use super::*;

#[test]
pub(super) fn test_lex_raw_fstring_case_variants() {
    // Upper/mixed-case prefix letters are accepted (Rf/rF/RF/Fr/fR/FR).
    for src in [
        "Rf\"x\"", "rF\"x\"", "RF\"x\"", "Fr\"x\"", "fR\"x\"", "FR\"x\"",
    ] {
        assert_eq!(
            lex_kinds(src),
            vec![TokenKind::RawFStr("x".into())],
            "src={src}"
        );
    }
}
