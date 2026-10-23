//! Python literal parse failures use the original token positions.
use mamba_language::application::quality_syntax::SyntaxDocument;

pub(super) fn reason(document: &SyntaxDocument<'_>) -> String {
    if document
        .source
        .strip_prefix("value = ")
        .is_some_and(str::is_empty)
    {
        return "invalid syntax (<unknown>, line 0)".into();
    }
    let mut delimiters = Vec::new();
    for token in &document.tokens {
        let symbol = match token.kind.as_str() {
            "Lsqb" => Some(('[', ']')),
            "Lpar" => Some(('(', ')')),
            "Lbrace" => Some(('{', '}')),
            _ => None,
        };
        if let Some(pair) = symbol {
            delimiters.push((pair, token.start));
        } else if matches!(token.kind.as_str(), "Rsqb" | "Rpar" | "Rbrace") {
            let close = document.text(token).chars().next().unwrap();
            let line = document.line_column(token.start).0;
            match delimiters.pop() {
                Some(((open, expected), offset)) if expected != close => {
                    let opening_line = document.line_column(offset).0;
                    let detail = if opening_line == line {
                        String::new()
                    } else {
                        format!(" on line {opening_line}")
                    };
                    return format!(
                        "closing parenthesis '{close}' does not match opening parenthesis '{open}'{detail} (<unknown>, line {line})"
                    );
                }
                None => return format!("unmatched '{close}' (<unknown>, line {line})"),
                _ => {}
            }
        }
    }
    if let Some(((open, _), offset)) = delimiters.last() {
        let line = document.line_column(*offset).0;
        return format!("'{open}' was never closed (<unknown>, line {line})");
    }
    let issue = document.issues.first();
    let line = issue.map_or(1, |issue| document.line_column(issue.start).0);
    let message = issue.map_or("", |issue| issue.message.as_str());
    let reason = if message.starts_with("Expected `,`,") {
        "invalid syntax. Perhaps you forgot a comma?"
    } else if message == "Invalid decimal integer literal" {
        "leading zeros in decimal integer literals are not permitted; use an 0o prefix for octal integers"
    } else if message.contains("after line continuation") {
        "unexpected character after line continuation character"
    } else if message == "Unexpected indentation" {
        "unexpected indent"
    } else {
        "invalid syntax"
    };
    if message == "Got unexpected unicode" {
        if let Some(reason) = unicode_error(document) {
            return format!("{reason} (<unknown>, line {line})");
        }
    }
    format!("{reason} (<unknown>, line {line})")
}

fn unicode_error(document: &SyntaxDocument<'_>) -> Option<String> {
    for token in &document.tokens {
        if token.kind != "String" {
            continue;
        }
        let raw = document.text(token);
        let first = raw.find(['\'', '"'])?;
        let quote = raw.as_bytes()[first];
        let quote_len = if raw
            .as_bytes()
            .get(first..first + 3)
            .is_some_and(|s| s == [quote; 3])
        {
            3
        } else {
            1
        };
        let body = &raw[first + quote_len..raw.len().saturating_sub(quote_len)];
        let bytes = body.as_bytes();
        let mut index = 0;
        while index + 1 < bytes.len() {
            if bytes[index] != b'\\' {
                index += 1;
                continue;
            }
            let kind = bytes[index + 1];
            let (length, description) = match kind {
                b'u' => (4, "\\uXXXX"),
                b'U' => (8, "\\UXXXXXXXX"),
                b'x' => (2, "\\xXX"),
                _ => {
                    index += 2;
                    continue;
                }
            };
            let valid = bytes[index + 2..]
                .iter()
                .take(length)
                .take_while(|b| b.is_ascii_hexdigit())
                .count();
            if valid != length {
                return Some(format!(
                    "(unicode error) 'unicodeescape' codec can't decode bytes in position {index}-{}: truncated {description} escape",
                    index + 1 + valid
                ));
            }
            index += length + 2;
        }
    }
    None
}
