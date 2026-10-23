use mamba_shared_kernel::token::BIG_INT_LITERAL_SENTINEL;

pub(super) fn is_dec_digit_byte(b: u8) -> bool {
    b.is_ascii_digit()
}

pub(super) fn is_hex_digit_byte(b: u8) -> bool {
    b.is_ascii_hexdigit()
}

pub(super) fn is_oct_digit_byte(b: u8) -> bool {
    matches!(b, b'0'..=b'7')
}

pub(super) fn is_bin_digit_byte(b: u8) -> bool {
    matches!(b, b'0' | b'1')
}

pub(super) fn underscores_between_digits(s: &str, is_digit: fn(u8) -> bool) -> bool {
    let bytes = s.as_bytes();
    for (i, b) in bytes.iter().enumerate() {
        if *b == b'_' {
            if i == 0 || i + 1 == bytes.len() {
                return false;
            }
            if !is_digit(bytes[i - 1]) || !is_digit(bytes[i + 1]) {
                return false;
            }
        }
    }
    true
}

pub(super) fn parse_decimal_int_literal(s: &str) -> Option<i64> {
    if !underscores_between_digits(s, is_dec_digit_byte) {
        return None;
    }
    let digits = s.replace('_', "");
    if digits.starts_with('0') && digits.chars().any(|c| c != '0') {
        return None;
    }
    Some(digits.parse::<i64>().unwrap_or(BIG_INT_LITERAL_SENTINEL))
}

pub(super) fn parse_decimal_float_literal(s: &str) -> Option<f64> {
    if !underscores_between_digits(s, is_dec_digit_byte) {
        return None;
    }
    s.replace('_', "").parse::<f64>().ok()
}

pub(super) fn parse_complex_literal(s: &str) -> Option<f64> {
    parse_decimal_float_literal(&s[..s.len() - 1])
}

pub(super) fn parse_prefixed_int_literal(
    s: &str,
    radix: u32,
    is_digit: fn(u8) -> bool,
) -> Option<i64> {
    let mut body = &s[2..];
    if let Some(rest) = body.strip_prefix('_') {
        body = rest;
    }
    if body.is_empty() || !underscores_between_digits(body, is_digit) {
        return None;
    }
    Some(i64::from_str_radix(&body.replace('_', ""), radix).unwrap_or(BIG_INT_LITERAL_SENTINEL))
}
