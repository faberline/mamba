/// Split f-string `{...}` content into expression and optional format spec.
/// The spec starts at the first `:` that is NOT inside brackets/parens/strings.
pub(super) fn split_expr_and_spec(raw: &str) -> (&str, Option<String>) {
    let bytes = raw.as_bytes();
    let mut depth = 0i32;
    let mut in_str: Option<u8> = None;
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
                b':' if depth == 0 => {
                    return (&raw[..i], Some(raw[i + 1..].to_string()));
                }
                _ => {}
            }
        }
        i += 1;
    }
    (raw, None)
}
