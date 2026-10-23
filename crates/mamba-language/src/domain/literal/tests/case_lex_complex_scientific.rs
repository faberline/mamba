use super::*;

#[test]
pub(super) fn test_lex_complex_scientific() {
    // `3.5e-20j` and friends were previously rejected — the
    // Complex regexes lacked the optional exponent (#1676). The
    // Float regex consumed `3.5e-20` and `j` fell through to an
    // identifier, producing a parser error.
    for (src, val) in [
        ("3.5e-20j", 3.5e-20_f64),
        ("2.0E5j", 2.0e5_f64),
        (".5e-2J", 0.005_f64),
        ("1e10j", 1e10_f64),
    ] {
        let kinds = lex_kinds(src);
        assert_eq!(kinds, vec![TokenKind::Complex(val)], "lexing {src}");
    }
}
