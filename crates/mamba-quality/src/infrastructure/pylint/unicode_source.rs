//! Raw source checks retain stream lines, character columns, and checker order.
use super::Checker;
use crate::domain::{model::QualityTool, source::SourceEncoding};

const BAD: &[(char, &str, &str, &str)] = &[
    ('\u{8}', "E2510", "backspace", "\\b"),
    ('\r', "E2511", "carriage-return", "\\r"),
    ('\u{1a}', "E2512", "sub", "\\x1A"),
    ('\u{1b}', "E2513", "esc", "\\x1B"),
    ('\0', "E2514", "nul", "\\0"),
    ('\u{200b}', "E2515", "zero-width-space", "\\u200B"),
];

impl Checker<'_> {
    pub(super) fn check_unicode_source(&mut self) {
        let utf8 = matches!(self.source.encoding, SourceEncoding::Utf8 { .. });
        if !utf8 {
            let line = encoding_line(&self.source.text);
            let mut diagnostic = super::super::diagnostics::at(
                self.source,
                QualityTool::Pylint,
                "C2503",
                "PEP8 recommends UTF-8 as encoding for Python files".into(),
                super::super::diagnostics::point(0),
            );
            diagnostic.location.line = line;
            diagnostic.location.column = 0;
            diagnostic.location.end_line = Some(line);
            if !super::directives::suppressed(&self.directives, "C2503", "bad-file-encoding", line)
            {
                self.diagnostics.push(diagnostic);
            }
        }
        let source = self.source.text.clone();
        let mut offset = 0;
        for (index, line) in source.split_inclusive('\n').enumerate() {
            let number = index as u32 + 1;
            let content = line.trim_end_matches('\n');
            let content = if line.ends_with('\n') {
                content.trim_end_matches('\r')
            } else {
                content
            };
            if utf8 && line.chars().any(|value| matches!(value, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}' | '\u{200f}')) {
                self.layout_diagnostic(
                    "E2502",
                    "bidirectional-unicode",
                    "Contains control characters that can permit obfuscated code executed differently than displayed",
                    number,
                    0,
                    Some(content.chars().count() as u32),
                    offset,
                );
            }
            // The reference groups matches by character kind, then by position.
            for &(character, rule, name, escape) in BAD {
                if line.starts_with(character) {
                    continue;
                }
                for (position, value) in
                    line.char_indices().filter(|(_, value)| *value == character)
                {
                    if value == '\r' && line[position..] == *"\r\n" {
                        continue;
                    }
                    let column = line[..position].chars().count() as u32 + 1;
                    self.layout_diagnostic(
                        rule,
                        &format!("invalid-character-{name}"),
                        &format!("Invalid unescaped character {name}, use \"{escape}\" instead."),
                        number,
                        column,
                        Some(column + 1),
                        offset + position,
                    );
                }
            }
            offset += line.len();
        }
    }
}

fn encoding_line(source: &str) -> u32 {
    static COOKIE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let cookie = COOKIE.get_or_init(|| {
        regex::Regex::new(r"^[ \t\x0c]*#.*?coding[=:][ \t]*[-_.a-zA-Z0-9]+")
            .expect("source encoding pattern")
    });
    source
        .split('\n')
        .take(2)
        .position(|line| cookie.is_match(line))
        .map_or(1, |index| index as u32 + 1)
}

fn declared_encoding(source: &[u8]) -> Option<String> {
    static COOKIE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let cookie = COOKIE.get_or_init(|| {
        regex::Regex::new(r"^[ \t\x0c]*#.*?coding[=:][ \t]*([-_.a-zA-Z0-9]+)")
            .expect("source encoding pattern")
    });
    let content = source.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(source);
    let mut lines = content.split(|byte| *byte == b'\n');
    let first = lines.next().unwrap_or_default();
    for (index, line) in std::iter::once(first).chain(lines.next()).enumerate() {
        if index == 1 && !second_encoding_line_possible(first) {
            break;
        }
        let ascii = String::from_utf8_lossy(line);
        if let Some(found) = cookie.captures(&ascii) {
            return Some(found[1].to_string());
        }
    }
    None
}

fn second_encoding_line_possible(first: &[u8]) -> bool {
    let first = String::from_utf8_lossy(first);
    let trimmed = first.trim_start_matches([' ', '\t', '\x0c']);
    trimmed.is_empty() || trimmed.starts_with(['#', '\r'])
}

fn invalid_utf8_during_encoding_detection(source: &[u8]) -> bool {
    let content = source.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(source);
    let mut lines = content.split(|byte| *byte == b'\n');
    let first = lines.next().unwrap_or_default();
    if std::str::from_utf8(first).is_err() {
        return true;
    }
    if declared_encoding(source).is_some() || !second_encoding_line_possible(first) {
        return false;
    }
    lines
        .next()
        .is_some_and(|line| std::str::from_utf8(line).is_err())
}

pub(super) fn token_failure(
    source: &crate::domain::source::SourceSnapshot,
    document: &mamba_language::application::quality_syntax::SyntaxDocument<'_>,
) -> Option<crate::domain::model::QualityDiagnostic> {
    if !source.text.contains('\r') {
        return None;
    }
    for (offset, value) in source.text.char_indices() {
        if !matches!(value, '\u{8}' | '\u{b}' | '\u{1a}' | '\u{1b}') {
            continue;
        }
        let before = &source.text[..offset];
        let start = before.rfind('\n').map_or(0, |index| index + 1);
        let in_literal = document.tokens.iter().any(|token| {
            token.start as usize <= offset
                && offset < token.end as usize
                && (token.kind == "String"
                    || (token.kind == "Comment"
                        && !source.text[start..token.start as usize].contains('\r')))
        });
        if in_literal || !source.text[start..offset].contains('\r') {
            continue;
        }
        let mut diagnostic = super::super::diagnostics::at(
            source,
            QualityTool::Pylint,
            "E0001",
            format!("invalid non-printable character U+{:04X}", value as u32),
            super::super::diagnostics::point(offset),
        );
        diagnostic.location.line =
            before.bytes().filter(|value| *value == b'\n').count() as u32 + 1;
        diagnostic.location.column = source.text[start..offset].chars().count() as u32 + 1;
        return Some(diagnostic);
    }
    None
}

pub(super) fn parsing_failure(
    source: &crate::domain::source::SourceSnapshot,
) -> Option<crate::domain::model::QualityDiagnostic> {
    let (rule, message) = if let Some(issue) = &source.decoding_issue {
        if issue.message.contains("utf-16")
            || issue.message.contains("utf-32")
            || issue.message.contains("UTF-16")
            || issue.message == "Non-ASCII bytes in an ASCII source file"
            || (issue.message.starts_with("Invalid UTF-8:")
                && !invalid_utf8_during_encoding_detection(&source.bytes))
        {
            (
                "F0010",
                format!(
                    "error while code parsing: Wrong or no encoding specified for {}.",
                    source.path.display()
                ),
            )
        } else if let Some(label) = issue.message.strip_prefix("Unsupported source encoding: ") {
            let declared = declared_encoding(&source.bytes).unwrap_or_else(|| label.to_string());
            (
                "E0001",
                format!(
                    "Parsing failed: 'unknown encoding for '{}': {declared}'",
                    source.path.display()
                ),
            )
        } else if issue.message == "UTF-8 BOM conflicts with the encoding declaration" {
            (
                "E0001",
                format!(
                    "Parsing failed: 'encoding problem for '{}': utf-8'",
                    source.path.display()
                ),
            )
        } else {
            (
                "E0001",
                format!(
                    "Parsing failed: 'invalid or missing encoding declaration for '{}''",
                    source.path.display()
                ),
            )
        }
    } else if source.text.contains('\0') {
        (
            "E0001",
            "Parsing failed: 'source code string cannot contain null bytes'".into(),
        )
    } else {
        return None;
    };
    Some(super::super::diagnostics::at(
        source,
        QualityTool::Pylint,
        rule,
        message,
        super::super::diagnostics::point(0),
    ))
}
