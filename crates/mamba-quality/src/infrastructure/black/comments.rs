//! Black 26.10.0 comment text. See assets/black/LICENSE.

fn python_whitespace(character: char) -> bool {
    // Python str.strip also treats these four C0 separators as whitespace.
    character.is_whitespace() || matches!(character, '\u{1c}'..='\u{1f}')
}

pub fn directive(comment: &str) -> bool {
    let comment = comment.trim_end_matches(python_whitespace);
    let direct = |block: &str| {
        matches!(
            block,
            "# fmt: off"
                | "# fmt:off"
                | "# fmt: on"
                | "# fmt:on"
                | "# fmt: skip"
                | "# fmt:skip"
                | "# yapf: disable"
                | "# yapf: enable"
        )
    };
    direct(comment)
        || comment
            .split("# ")
            .skip(1)
            .any(|block| direct(&format!("# {}", block.trim_matches(python_whitespace))))
        || comment
            .trim_matches(['#', ' '])
            .split(';')
            .any(|block| direct(&format!("# {}", block.trim_matches(python_whitespace))))
}

pub fn normalize(source: &str) -> String {
    let source = source.trim_end_matches(python_whitespace);
    if source.is_empty() {
        return "#".into();
    }
    if source.starts_with('#') && directive(source) {
        return source.into();
    }
    let mut content = source.strip_prefix('#').unwrap_or(source).to_string();
    if content.starts_with('\u{a0}')
        && !content
            .trim_start_matches(python_whitespace)
            .starts_with("type:")
    {
        content.replace_range(..'\u{a0}'.len_utf8(), " ");
    }
    if !content.contains('\u{a0}')
        && content
            .trim_start_matches(python_whitespace)
            .starts_with("type:")
    {
        let (kind, value) = content.split_once(':').unwrap();
        content = format!(
            "{}: {}",
            kind.trim_matches(python_whitespace),
            value.trim_matches(python_whitespace)
        );
    }
    if content
        .chars()
        .next()
        .is_some_and(|character| !matches!(character, ' ' | '!' | ':' | '#' | '\''))
    {
        content.insert(0, ' ');
    }
    format!("#{content}")
}
