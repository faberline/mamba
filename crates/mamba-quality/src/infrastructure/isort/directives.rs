use mamba_language::application::quality_syntax::SyntaxDocument;

pub fn skip_file(text: &str) -> bool {
    // The reference scans these file-skip markers before quote handling.
    ["isort:skip_file", "isort: skip_file"]
        .iter()
        .any(|marker| text.contains(marker))
}

pub fn enabled(document: &SyntaxDocument<'_>, start: usize, end: usize) -> bool {
    let mut enabled = true;
    for token in document
        .tokens
        .iter()
        .filter(|token| token.kind == "Comment")
    {
        let offset = token.start as usize;
        let comment = document.text(token).trim();
        if offset < start {
            let prefix = &document.source[..offset];
            let prefix = prefix.rsplit(['\r', '\n']).next().unwrap_or_default();
            if prefix.trim().is_empty() {
                if comment == "# isort: off" {
                    enabled = false;
                }
                if comment == "# isort: on" {
                    enabled = true;
                }
            }
        } else if offset < end
            && (comment.contains("isort:skip")
                || comment.contains("isort: skip")
                || comment.contains("isort: split"))
        {
            return false;
        }
    }
    enabled
}

pub fn restart_prefix(document: &SyntaxDocument<'_>, start: usize) -> (usize, String) {
    if start == 0 {
        return (start, String::new());
    }
    let text = document.source;
    let previous_end = start;
    let previous_start = text[..previous_end.saturating_sub(1)]
        .rfind('\n')
        .map_or(0, |offset| offset + 1);
    if text[previous_start..previous_end].trim() != "# isort: on"
        || enabled(document, previous_start, previous_start)
    {
        return (start, String::new());
    }
    let before = text[..previous_start].trim_end_matches(['\r', '\n']);
    let has_content = !before.is_empty()
        && !text[..previous_start].ends_with("\n\n")
        && !text[..previous_start].ends_with("\r\n\r\n");
    let mut prefix = if has_content {
        super::model::newline(text).to_owned()
    } else {
        String::new()
    };
    prefix.push_str(&text[previous_start..previous_end]);
    (previous_start, prefix)
}
