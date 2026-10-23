use super::*;

/// Process all Python escape sequences in a regular string literal.
/// Handles: `\\`, `\'`, `\"`, `\n`, `\t`, `\r`, `\a`, `\b`, `\f`, `\v`, `\0`,
/// `\ooo` (octal), `\xHH`, `\uXXXX`, `\UXXXXXXXX`, `\N{name}`.
pub(crate) fn apply_escape_sequences(s: &str) -> String {
    let mut result = String::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\\' {
            result.push(c);
            continue;
        }
        match chars.peek().copied() {
            Some('\\') => {
                chars.next();
                result.push('\\');
            }
            // Line continuation: backslash before a literal newline collapses
            // to nothing (also swallowing a CR in CRLF sources).
            Some('\n') => {
                chars.next();
            }
            Some('\r') => {
                chars.next();
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
            }
            Some('\'') => {
                chars.next();
                result.push('\'');
            }
            Some('"') => {
                chars.next();
                result.push('"');
            }
            Some('n') => {
                chars.next();
                result.push('\n');
            }
            Some('t') => {
                chars.next();
                result.push('\t');
            }
            Some('r') => {
                chars.next();
                result.push('\r');
            }
            Some('a') => {
                chars.next();
                result.push('\x07');
            }
            Some('b') => {
                chars.next();
                result.push('\x08');
            }
            Some('f') => {
                chars.next();
                result.push('\x0C');
            }
            Some('v') => {
                chars.next();
                result.push('\x0B');
            }
            Some('0') => {
                chars.next();
                result.push('\0');
            }
            Some('N') => {
                chars.next();
                if chars.peek() == Some(&'{') {
                    chars.next();
                    let mut name = String::new();
                    while let Some(&nc) = chars.peek() {
                        if nc == '}' {
                            chars.next();
                            break;
                        }
                        name.push(nc);
                        chars.next();
                    }
                    if let Some(uc) = unicode_name_to_char(&name) {
                        result.push(uc);
                    } else {
                        result.push('\\');
                        result.push('N');
                        result.push('{');
                        result.push_str(&name);
                        result.push('}');
                    }
                } else {
                    result.push('\\');
                    result.push('N');
                }
            }
            Some('u') => {
                chars.next();
                let mut hex = String::with_capacity(4);
                for _ in 0..4 {
                    if let Some(h) = chars.next() {
                        hex.push(h);
                    }
                }
                if let Ok(n) = u32::from_str_radix(&hex, 16) {
                    if (0xD800..=0xDFFF).contains(&n) {
                        push_surrogate_escape_marker(&mut result, n);
                        continue;
                    }
                    if let Some(uc) = char::from_u32(n) {
                        result.push(uc);
                        continue;
                    }
                }
                result.push('\\');
                result.push('u');
                result.push_str(&hex);
            }
            Some('U') => {
                chars.next();
                let mut hex = String::with_capacity(8);
                for _ in 0..8 {
                    if let Some(h) = chars.next() {
                        hex.push(h);
                    }
                }
                if let Ok(n) = u32::from_str_radix(&hex, 16) {
                    if (0xD800..=0xDFFF).contains(&n) {
                        push_surrogate_escape_marker(&mut result, n);
                        continue;
                    }
                    if let Some(uc) = char::from_u32(n) {
                        result.push(uc);
                        continue;
                    }
                }
                result.push('\\');
                result.push('U');
                result.push_str(&hex);
            }
            Some('x') => {
                chars.next();
                let mut hex = String::with_capacity(2);
                for _ in 0..2 {
                    if let Some(h) = chars.next() {
                        hex.push(h);
                    }
                }
                if let Ok(n) = u32::from_str_radix(&hex, 16) {
                    if let Some(uc) = char::from_u32(n) {
                        result.push(uc);
                        continue;
                    }
                }
                result.push('\\');
                result.push('x');
                result.push_str(&hex);
            }
            Some(d) if d.is_ascii_digit() && d != '8' && d != '9' => {
                // Octal escape: \ooo (up to 3 octal digits)
                chars.next();
                let mut oct = String::with_capacity(3);
                oct.push(d);
                for _ in 0..2 {
                    if let Some(&nd) = chars.peek() {
                        if nd.is_ascii_digit() && nd != '8' && nd != '9' {
                            oct.push(nd);
                            chars.next();
                        } else {
                            break;
                        }
                    }
                }
                if let Ok(n) = u32::from_str_radix(&oct, 8) {
                    if let Some(uc) = char::from_u32(n) {
                        result.push(uc);
                        continue;
                    }
                }
                result.push('\\');
                result.push_str(&oct);
            }
            _ => result.push(c),
        }
    }
    result
}
