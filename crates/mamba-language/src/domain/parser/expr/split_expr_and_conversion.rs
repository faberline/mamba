/// Peel an optional `!r` / `!s` / `!a` conversion suffix off an f-string
/// replacement-field expression. Only detects a trailing `!<letter>` at the
/// top level (ignores `!` inside brackets/strings and `!=` operators).
/// Returns (expr_without_conversion, conversion_char).
pub(super) fn split_expr_and_conversion(raw: &str) -> (&str, Option<char>) {
    let bytes = raw.as_bytes();
    // Find the last top-level `!` followed by a single ASCII alpha at end-of-string.
    // We scan forward tracking bracket/string depth and record the latest candidate.
    let mut depth = 0i32;
    let mut in_str: Option<u8> = None;
    let mut candidate: Option<(usize, char)> = None;
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if let Some(q) = in_str {
            if b == b'\\' {
                i += if i + 1 < bytes.len() { 2 } else { 1 };
                continue;
            }
            if b == q {
                in_str = None;
            }
        } else {
            match b {
                b'\'' | b'"' => in_str = Some(b),
                b'(' | b'[' | b'{' => depth += 1,
                b')' | b']' | b'}' => depth -= 1,
                b'!' if depth == 0 => {
                    // Must not be `!=` (inequality).
                    let next = bytes.get(i + 1).copied();
                    if next != Some(b'=') {
                        if let Some(c) = next {
                            if c.is_ascii_alphabetic() && bytes.get(i + 2).is_none() {
                                candidate = Some((i, c as char));
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        i += 1;
    }
    if let Some((pos, c)) = candidate {
        (&raw[..pos], Some(c))
    } else {
        (raw, None)
    }
}
