/// Process backslash escapes in an f-string literal run unless the f-string is
/// raw (`rf'...'` / `fr'...'`).  `{{` / `}}` have already been collapsed by the
/// caller; only true backslash escapes (`\n`, `\t`, `\\`, ...) remain.
pub(super) fn finish_fstring_literal(lit: String, is_raw: bool) -> String {
    if is_raw {
        lit
    } else {
        crate::domain::literal::apply_escape_sequences(&lit)
    }
}
