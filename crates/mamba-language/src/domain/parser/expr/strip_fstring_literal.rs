/// If `s` is a standalone f-string literal (`f"..."` or `f'...'`), return
/// the inner content (between the quotes).  Returns `None` for anything
/// more complex (e.g. `f'{x}' + "y"`).  Handles matching quote pairs only.
pub(super) fn strip_fstring_literal(s: &str) -> Option<&str> {
    let bytes = s.as_bytes();
    if bytes.len() < 3 || bytes[0] != b'f' {
        return None;
    }
    let quote = bytes[1];
    if quote != b'\'' && quote != b'"' {
        return None;
    }
    if *bytes.last()? != quote {
        return None;
    }
    // Verify no unmatched quotes between start and end: walk the inner
    // content and ensure we don't exit the string prematurely.
    let inner = &s[2..s.len() - 1];
    let ib = inner.as_bytes();
    let mut depth = 0i32;
    let mut in_str: Option<u8> = None;
    let mut j = 0;
    while j < ib.len() {
        let b = ib[j];
        if let Some(q) = in_str {
            if b == b'\\' {
                j += if j + 1 < ib.len() { 2 } else { 1 };
                continue;
            }
            if b == q {
                in_str = None;
            }
        } else {
            match b {
                b'{' => depth += 1,
                b'}' => depth -= 1,
                b'\'' | b'"' => {
                    if b == quote {
                        // Unmatched outer quote in the middle → not a simple f-string literal
                        return None;
                    }
                    in_str = Some(b);
                }
                _ => {}
            }
        }
        j += 1;
    }
    if depth != 0 {
        return None;
    }
    Some(inner)
}
