//! Original isort quote scanning determines which physical lines are literal data.
pub(super) fn has_open_quote(line: &str, state: &mut String) {
    let bytes = line.as_bytes();
    let stripped = line.trim();
    if !((!stripped.starts_with('#') || !state.is_empty()) && line.contains('"')
        || line.contains('\''))
    {
        return;
    }
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'\\' {
            index += 1;
        } else if !state.is_empty() {
            if bytes[index..].starts_with(state.as_bytes()) {
                state.clear();
            }
        } else if matches!(bytes[index], b'\'' | b'"') {
            let quote = bytes[index];
            if bytes.get(index..index + 3).is_some_and(|s| s == [quote; 3]) {
                *state = (quote as char).to_string().repeat(3);
                index += 2;
            } else {
                *state = (quote as char).to_string();
            }
        } else if bytes[index] == b'#' {
            break;
        }
        index += 1;
    }
}

pub(super) fn collected(raw: &str) -> (String, String) {
    let mut quote = String::new();
    let mut code = String::new();
    let mut preserved = String::new();
    for line in raw.split_inclusive('\n') {
        let was_quoted = !quote.is_empty();
        has_open_quote(line, &mut quote);
        if was_quoted || !quote.is_empty() {
            preserved.push_str(line);
        } else {
            code.push_str(line);
        }
    }
    (code, preserved)
}
