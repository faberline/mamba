/// Process escape sequences in a byte string literal, producing Vec<u8>.
/// Supports: `\\`, `\'`, `\"`, `\n`, `\t`, `\r`, `\a`, `\b`, `\f`, `\v`, `\0`,
/// `\ooo` (octal), `\xHH`. Does NOT support `\u`, `\U`, `\N{name}`.
pub(crate) fn apply_bytes_escapes(s: &str) -> Vec<u8> {
    let mut result = Vec::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\\' {
            // Only allow ASCII bytes in byte strings
            if c.is_ascii() {
                result.push(c as u8);
            } else {
                // Non-ASCII: encode as UTF-8 bytes
                let mut buf = [0u8; 4];
                let encoded = c.encode_utf8(&mut buf);
                result.extend_from_slice(encoded.as_bytes());
            }
            continue;
        }
        match chars.peek().copied() {
            Some('\\') => {
                chars.next();
                result.push(b'\\');
            }
            Some('\'') => {
                chars.next();
                result.push(b'\'');
            }
            Some('"') => {
                chars.next();
                result.push(b'"');
            }
            Some('n') => {
                chars.next();
                result.push(b'\n');
            }
            Some('t') => {
                chars.next();
                result.push(b'\t');
            }
            Some('r') => {
                chars.next();
                result.push(b'\r');
            }
            Some('a') => {
                chars.next();
                result.push(0x07);
            }
            Some('b') => {
                chars.next();
                result.push(0x08);
            }
            Some('f') => {
                chars.next();
                result.push(0x0C);
            }
            Some('v') => {
                chars.next();
                result.push(0x0B);
            }
            Some('0') => {
                chars.next();
                result.push(0);
            }
            Some('x') => {
                chars.next();
                let mut hex = String::with_capacity(2);
                for _ in 0..2 {
                    if let Some(h) = chars.next() {
                        hex.push(h);
                    }
                }
                if let Ok(n) = u8::from_str_radix(&hex, 16) {
                    result.push(n);
                } else {
                    result.push(b'\\');
                    result.push(b'x');
                    result.extend_from_slice(hex.as_bytes());
                }
            }
            Some(d) if d.is_ascii_digit() && d != '8' && d != '9' => {
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
                if let Ok(n) = u8::from_str_radix(&oct, 8) {
                    result.push(n);
                } else {
                    result.push(b'\\');
                    result.extend_from_slice(oct.as_bytes());
                }
            }
            _ => {
                result.push(b'\\');
                if let Some(nc) = chars.next() {
                    if nc.is_ascii() {
                        result.push(nc as u8);
                    }
                }
            }
        }
    }
    result
}
