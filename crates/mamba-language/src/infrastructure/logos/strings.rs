use super::kind::LogosKind;
use crate::domain::literal::apply_escape_sequences;

pub(super) fn lex_triple_dquote(lex: &mut logos::Lexer<LogosKind>) -> Option<String> {
    let remainder = lex.remainder();
    let bytes = remainder.as_bytes();
    let mut i = 0;
    while i < remainder.len() {
        if i + 2 < remainder.len()
            && bytes[i] == b'"'
            && bytes[i + 1] == b'"'
            && bytes[i + 2] == b'"'
        {
            let content = remainder[..i].to_string();
            lex.bump(i + 3);
            return Some(content);
        }
        if bytes[i] == b'\\' && i + 1 < remainder.len() {
            i += 2;
        } else {
            i += 1;
        }
    }
    None
}

pub(super) fn lex_triple_squote(lex: &mut logos::Lexer<LogosKind>) -> Option<String> {
    let remainder = lex.remainder();
    let bytes = remainder.as_bytes();
    let mut i = 0;
    while i < remainder.len() {
        if i + 2 < remainder.len()
            && bytes[i] == b'\''
            && bytes[i + 1] == b'\''
            && bytes[i + 2] == b'\''
        {
            let content = remainder[..i].to_string();
            lex.bump(i + 3);
            return Some(content);
        }
        if bytes[i] == b'\\' && i + 1 < remainder.len() {
            i += 2;
        } else {
            i += 1;
        }
    }
    None
}

pub(super) fn lex_regular_triple_dquote(lex: &mut logos::Lexer<LogosKind>) -> Option<String> {
    lex_triple_dquote(lex).map(|content| apply_escape_sequences(&content))
}

pub(super) fn lex_regular_triple_squote(lex: &mut logos::Lexer<LogosKind>) -> Option<String> {
    lex_triple_squote(lex).map(|content| apply_escape_sequences(&content))
}

// Raw triple-quoted variants (#1678): scan to the matching `"""`/`'''` but
// pass backslashes through literally (no escape processing).
pub(super) fn lex_raw_triple_dquote(lex: &mut logos::Lexer<LogosKind>) -> Option<String> {
    let remainder = lex.remainder();
    let bytes = remainder.as_bytes();
    let mut i = 0;
    while i + 2 < remainder.len() {
        if bytes[i] == b'"' && bytes[i + 1] == b'"' && bytes[i + 2] == b'"' {
            let content = remainder[..i].to_string();
            lex.bump(i + 3);
            return Some(content);
        }
        i += 1;
    }
    None
}

pub(super) fn lex_raw_triple_squote(lex: &mut logos::Lexer<LogosKind>) -> Option<String> {
    let remainder = lex.remainder();
    let bytes = remainder.as_bytes();
    let mut i = 0;
    while i + 2 < remainder.len() {
        if bytes[i] == b'\'' && bytes[i + 1] == b'\'' && bytes[i + 2] == b'\'' {
            let content = remainder[..i].to_string();
            lex.bump(i + 3);
            return Some(content);
        }
        i += 1;
    }
    None
}

// --- PEP 701 f-string callbacks ---

/// Lex a double-quoted f-string (PEP 701): called after `f"` is consumed.
pub(super) fn lex_fstr_dquote(lex: &mut logos::Lexer<LogosKind>) -> Option<String> {
    lex_fstr_inner(lex, b'"')
}

/// Lex a single-quoted f-string (PEP 701): called after `f'` is consumed.
pub(super) fn lex_fstr_squote(lex: &mut logos::Lexer<LogosKind>) -> Option<String> {
    lex_fstr_inner(lex, b'\'')
}

/// Lex a triple-double-quoted f-string: called after `f"""` is consumed.
pub(super) fn lex_fstr_triple_dquote(lex: &mut logos::Lexer<LogosKind>) -> Option<String> {
    lex_fstr_inner_triple(lex, b'"')
}

/// Lex a triple-single-quoted f-string: called after `f'''` is consumed.
pub(super) fn lex_fstr_triple_squote(lex: &mut logos::Lexer<LogosKind>) -> Option<String> {
    lex_fstr_inner_triple(lex, b'\'')
}

/// Triple-quoted f-string lexer: the body only closes on THREE consecutive
/// quote chars at body level; single/double quotes inside replacement-field
/// expressions nest freely (`f"""{"eric's"}"""`). Same stack discipline as
/// `lex_fstr_inner`.
pub(super) fn lex_fstr_inner_triple(
    lex: &mut logos::Lexer<LogosKind>,
    close_quote: u8,
) -> Option<String> {
    let remainder = lex.remainder();
    let bytes = remainder.as_bytes();
    let mut i = 0usize;
    let mut stack: Vec<u8> = Vec::new();

    while i < remainder.len() {
        let b = bytes[i];
        if stack.is_empty() {
            if b == close_quote
                && bytes.get(i + 1) == Some(&close_quote)
                && bytes.get(i + 2) == Some(&close_quote)
            {
                let content = remainder[..i].to_string();
                lex.bump(i + 3);
                return Some(content);
            } else if b == b'{' {
                if bytes.get(i + 1) == Some(&b'{') {
                    i += 2; // {{ escape
                } else {
                    stack.push(b'e');
                    i += 1;
                }
            } else if b == b'}' {
                i += if bytes.get(i + 1) == Some(&b'}') {
                    2
                } else {
                    1
                };
            } else if b == b'\\' {
                i += if i + 1 < remainder.len() { 2 } else { 1 };
            } else {
                i += 1;
            }
        } else {
            let top = *stack.last().unwrap();
            if top == b'e' {
                if b == b'}' {
                    stack.pop();
                    i += 1;
                } else if b == b'{' {
                    stack.push(b'e');
                    i += 1;
                } else if b == b'"' || b == b'\'' {
                    stack.push(b);
                    i += 1;
                } else if b == b'\\' {
                    i += if i + 1 < remainder.len() { 2 } else { 1 };
                } else {
                    i += 1;
                }
            } else if b == top {
                stack.pop();
                i += 1;
            } else if b == b'\\' {
                i += if i + 1 < remainder.len() { 2 } else { 1 };
            } else {
                i += 1;
            }
        }
    }
    None // unterminated
}

/// Shared f-string lexer. `close_quote` is `b'"'` or `b'\''`.
///
/// Stack entries: `b'e'` = inside `{expr}`, `b'"'` = double-quoted nested
/// string, `b'\''` = single-quoted nested string.  Pushing `b'e'` on `{`
/// even from inside a nested string layer handles deeply nested f-strings
/// such as `f"{f"{f"deep"}"}"`.
pub(super) fn lex_fstr_inner(lex: &mut logos::Lexer<LogosKind>, close_quote: u8) -> Option<String> {
    let remainder = lex.remainder();
    let bytes = remainder.as_bytes();
    let mut i = 0usize;
    let mut stack: Vec<u8> = Vec::new();

    while i < remainder.len() {
        let b = bytes[i];
        if stack.is_empty() {
            // ── f-string body ─────────────────────────────────────────────────────────────
            if b == close_quote {
                let content = remainder[..i].to_string();
                lex.bump(i + 1);
                return Some(content);
            } else if b == b'{' {
                if i + 1 < remainder.len() && bytes[i + 1] == b'{' {
                    i += 2; // {{ escape
                } else {
                    stack.push(b'e');
                    i += 1;
                }
            } else if b == b'}' {
                if i + 1 < remainder.len() && bytes[i + 1] == b'}' {
                    i += 2; // }} escape
                } else {
                    i += 1;
                }
            } else if b == b'\\' {
                i += if i + 1 < remainder.len() { 2 } else { 1 };
            } else {
                i += 1;
            }
        } else {
            let top = *stack.last().unwrap();
            if top == b'e' {
                // ── inside expression ─────────────────────────────────────────────────
                if b == b'}' {
                    stack.pop();
                    i += 1;
                } else if b == b'{' {
                    stack.push(b'e');
                    i += 1;
                } else if b == b'"' {
                    stack.push(b'"');
                    i += 1;
                } else if b == b'\'' {
                    stack.push(b'\'');
                    i += 1;
                } else if b == b'\\' {
                    // PEP 701: backslashes allowed in expressions
                    i += if i + 1 < remainder.len() { 2 } else { 1 };
                } else {
                    i += 1;
                }
            } else {
                // ── inside nested string ────────────────────────────────────────────────
                // `{` inside a plain string literal (e.g. `'{'` in `f"{'{'}"`)
                // is NOT an expression start.  Only the outer expression-level
                // `{...}` braces need to be tracked.  The original code that
                // pushed `b'e'` here was incorrect for PEP-701 (Python 3.12+).
                if b == top {
                    stack.pop();
                    i += 1;
                } else if b == b'\\' {
                    i += if i + 1 < remainder.len() { 2 } else { 1 };
                } else {
                    i += 1;
                }
            }
        }
    }
    None // unterminated
}
