//! Black stable format-directive recognition. See LICENSE.black.
use ruff_python_trivia::SuppressionKind;

fn python_whitespace(character: char) -> bool {
    // Black make_comment calls Python rstrip before it matches directives.
    character.is_whitespace() || matches!(character, '\u{1c}'..='\u{1f}')
}

fn kind(block: &str) -> Option<SuppressionKind> {
    match block {
        "# fmt: off" | "# fmt:off" | "# yapf: disable" => Some(SuppressionKind::Off),
        "# fmt: on" | "# fmt:on" | "# yapf: enable" => Some(SuppressionKind::On),
        "# fmt: skip" | "# fmt:skip" => Some(SuppressionKind::Skip),
        _ => None,
    }
}

pub(crate) fn controls(comment: &str) -> Vec<SuppressionKind> {
    let mut result = Vec::new();
    fn record(result: &mut Vec<SuppressionKind>, block: &str) {
        if let Some(kind) = kind(block) {
            if !result.contains(&kind) {
                result.push(kind);
            }
        }
    }
    record(&mut result, comment.trim_end_matches(python_whitespace));
    for block in comment.split("# ").skip(1) {
        record(
            &mut result,
            &format!("# {}", block.trim_matches(python_whitespace)),
        );
    }
    for block in comment.trim_matches(['#', ' ']).split(';') {
        record(
            &mut result,
            &format!("# {}", block.trim_matches(python_whitespace)),
        );
    }
    // Black normalizes a lone comment before interpreting a missing space.
    if result.is_empty() {
        let raw = comment
            .strip_prefix('#')
            .unwrap_or(comment)
            .trim_end_matches(python_whitespace);
        record(
            &mut result,
            &format!("# {}", raw.trim_start_matches(python_whitespace)),
        );
    }
    result
}
