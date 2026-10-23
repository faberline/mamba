//! Port of Black 26.10.0 literal normalization. See assets/black/LICENSE.
use regex::{Captures, Regex};
use std::sync::OnceLock;

fn float(text: &str) -> String {
    if let Some((before, after)) = text.split_once('.') {
        format!(
            "{}.{}",
            if before.is_empty() { "0" } else { before },
            if after.is_empty() { "0" } else { after }
        )
    } else {
        text.into()
    }
}

pub fn number(source: &str) -> String {
    let text = source.to_ascii_lowercase();
    if text.starts_with("0o") || text.starts_with("0b") {
        text
    } else if let Some(hex) = text.strip_prefix("0x") {
        format!("0x{}", hex.to_ascii_uppercase())
    } else if let Some((before, after)) = text.split_once('e') {
        format!(
            "{}e{}",
            float(before),
            after.strip_prefix('+').unwrap_or(after)
        )
    } else if let Some(before) = text.strip_suffix('j') {
        format!("{}j", float(before))
    } else {
        float(&text)
    }
}

fn unicode_escapes(source: &str, raw: bool) -> String {
    if raw {
        return source.into();
    }
    static ESCAPES: OnceLock<Regex> = OnceLock::new();
    let pattern = ESCAPES.get_or_init(|| Regex::new(
        r"(?P<slashes>\\+)(?P<body>u(?P<u>[a-fA-F0-9]{4})|U(?P<U>[a-fA-F0-9]{8})|x(?P<x>[a-fA-F0-9]{2})|N\{(?P<N>[a-zA-Z0-9 \-]{2,})\})?"
    ).expect("fixed Black escape pattern"));
    pattern
        .replace_all(source, |captures: &Captures<'_>| {
            let slashes = &captures["slashes"];
            if slashes.len().is_multiple_of(2) || captures.name("body").is_none() {
                return captures[0].into();
            }
            for key in ["u", "U", "x"] {
                if let Some(value) = captures.name(key) {
                    return format!("{slashes}{key}{}", value.as_str().to_ascii_lowercase());
                }
            }
            format!("{slashes}N{{{}}}", captures["N"].to_ascii_uppercase())
        })
        .into_owned()
}

fn prefix(source: &str, start: usize) -> String {
    let mut result = source[..start]
        .replace(['u', 'U'], "")
        .replace('F', "f")
        .replace('B', "b");
    if result.len() == 2 && !result.starts_with(['r', 'R']) {
        result = result.chars().rev().collect();
    }
    result
}

enum EscapeEdit {
    Remove,
    Add,
}

fn edit_quotes(body: &str, quote: &str, edit: EscapeEdit) -> String {
    let mut output = String::new();
    let mut offset = 0;
    while offset < body.len() {
        let start = offset;
        while body.as_bytes().get(offset) == Some(&b'\\') {
            offset += 1;
        }
        let slashes = offset - start;
        if body[offset..].starts_with(quote) {
            let count = match edit {
                EscapeEdit::Remove if slashes % 2 == 1 => slashes - 1,
                EscapeEdit::Add if slashes % 2 == 0 => slashes + 1,
                _ => slashes,
            };
            output.extend(std::iter::repeat_n('\\', count));
            output.push_str(quote);
            offset += quote.len();
        } else {
            output.push_str(&body[start..offset]);
            if let Some(character) = body[offset..].chars().next() {
                output.push(character);
                offset += character.len_utf8();
            }
        }
    }
    output
}

fn has_unescaped_quote(body: &str, quote: &str) -> bool {
    let mut offset = 0;
    while offset < body.len() {
        let start = offset;
        while body.as_bytes().get(offset) == Some(&b'\\') {
            offset += 1;
        }
        if (offset - start).is_multiple_of(2) && body[offset..].starts_with(quote) {
            return true;
        }
        if let Some(character) = body[offset..].chars().next() {
            offset += character.len_utf8();
        }
    }
    false
}

// Match Black's stable interpolation guard, including its one-line restriction.
fn escaped_interpolation(body: &str) -> bool {
    let bytes = body.as_bytes();
    for (start, character) in body.char_indices() {
        if character != '{'
            || start > 0 && bytes[start - 1] == b'{'
            || bytes.get(start + 1) == Some(&b'{')
        {
            continue;
        }
        let rest = &body[start + 1..];
        for (end, character) in rest.char_indices() {
            if character == '\n' {
                break;
            }
            if character == '}' && rest.as_bytes().get(end + 1) != Some(&b'}') {
                if rest[..end].contains('\\') {
                    return true;
                }
                break;
            }
        }
    }
    false
}

pub fn string(source: &str) -> String {
    let Some(start) = source.find(['\'', '"']) else {
        return source.into();
    };
    let raw = source[..start].contains(['r', 'R']);
    let source = unicode_escapes(source, raw);
    let normalized_prefix = prefix(&source, start);
    let value = &source[start..];
    let (old, new) = if value.starts_with("\"\"\"") {
        return format!("{normalized_prefix}{value}");
    } else if value.starts_with("'''") {
        ("'''", "\"\"\"")
    } else if value.starts_with('"') {
        ("\"", "'")
    } else {
        ("'", "\"")
    };
    let original_body = &value[old.len()..value.len() - old.len()];
    let body;
    let mut candidate;
    if raw {
        if has_unescaped_quote(original_body, new) {
            return format!("{normalized_prefix}{value}");
        }
        body = original_body.to_string();
        candidate = body.clone();
    } else {
        body = edit_quotes(original_body, new, EscapeEdit::Remove);
        candidate = edit_quotes(&body, old, EscapeEdit::Remove);
        candidate = edit_quotes(&candidate, new, EscapeEdit::Add);
    }
    let original = format!("{normalized_prefix}{old}{body}{old}");
    if normalized_prefix.contains(['f', 'F', 't', 'T']) && escaped_interpolation(&candidate) {
        return original;
    }
    if new == "\"\"\"" && candidate.ends_with('"') {
        let before = &candidate[..candidate.len() - 1];
        let backslashes = before.len() - before.trim_end_matches('\\').len();
        if backslashes.is_multiple_of(2) {
            candidate.pop();
            candidate.push_str("\\\"");
        }
    }
    let old_count = body.bytes().filter(|byte| *byte == b'\\').count();
    let new_count = candidate.bytes().filter(|byte| *byte == b'\\').count();
    if new_count > old_count || new_count == old_count && old == "\"" {
        original
    } else {
        format!("{normalized_prefix}{new}{candidate}{new}")
    }
}
