//! Source layout preserves physical lines and token-owned whitespace exemptions.
use super::{directives::suppressed, Checker};
use crate::domain::model::QualityTool;
use mamba_language::application::quality_syntax::SyntaxDocument;
pub(super) fn whitespace(value: char) -> bool {
    value.is_whitespace() || matches!(value, '\u{1c}'..='\u{1f}')
}
pub(super) fn lines(source: &str) -> Vec<(usize, &str)> {
    let mut output = Vec::new();
    let mut start = 0;
    let bytes = source.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\r' || bytes[i] == b'\n' {
            i += 1;
            if bytes[i - 1] == b'\r' && bytes.get(i) == Some(&b'\n') {
                i += 1;
            }
            output.push((start, &source[start..i]));
            start = i;
        } else {
            i += 1;
        }
    }
    if start < source.len() {
        output.push((start, &source[start..]));
    }
    output
}
impl Checker<'_> {
    pub(super) fn layout_diagnostic(
        &mut self,
        rule: &str,
        name: &str,
        message: &str,
        line: u32,
        column: u32,
        end: Option<u32>,
        offset: usize,
    ) {
        let mut value = super::super::diagnostics::at(
            self.source,
            QualityTool::Pylint,
            rule,
            message.into(),
            super::super::diagnostics::point(offset),
        );
        value.location.line = line;
        value.location.column = column;
        value.location.end_line = end.map(|_| line);
        value.location.end_column = end.map(i64::from);
        if !suppressed(&self.directives, rule, name, line) {
            self.diagnostics.push(value);
        } else {
            self.record_suppression(rule, name, line);
        }
    }
    pub(super) fn check_source_layout(&mut self, document: Option<&SyntaxDocument<'_>>) {
        let tokens = super::layout_tokens::Lines::new(document);
        let url = long_url();
        let indent = document
            .map(super::indentation::expected)
            .unwrap_or_default();
        let mut ending = None;
        let mut count = 0;
        let mut last_blank = false;
        let source = self.source.text.clone();
        let strings: Vec<_> = document
            .into_iter()
            .flat_map(|value| value.tokens.iter())
            .filter(|value| value.kind == "String")
            .collect();
        for (index, (offset, line)) in lines(&source).into_iter().enumerate() {
            count = index + 1;
            let content = line.trim_end_matches(['\r', '\n']);
            let stripped = content.trim_end_matches(whitespace);
            let logical = tokens
                .newlines
                .range(offset..offset + line.len())
                .next()
                .is_some();
            let last = tokens.semicolons.contains(&(count as u32));
            if last {
                self.layout_diagnostic(
                    "W0301",
                    "unnecessary-semicolon",
                    "Unnecessary semicolon",
                    count as u32,
                    0,
                    None,
                    offset,
                );
            }
            let length = line_length(stripped);
            if length > 88 && !url.is_match(stripped) {
                self.layout_diagnostic(
                    "C0301",
                    "line-too-long",
                    &format!("Line too long ({length}/88)"),
                    count as u32,
                    0,
                    None,
                    offset,
                );
            }
            if !line.ends_with('\n') {
                self.layout_diagnostic(
                    "C0304",
                    "missing-final-newline",
                    "Final newline missing",
                    count as u32,
                    0,
                    None,
                    offset,
                );
            } else {
                let first = content
                    .char_indices()
                    .find(|(_, value)| !matches!(value, ' ' | '\t' | '\x0c'))
                    .map_or(offset, |(index, _)| offset + index);
                let string_line = strings
                    .iter()
                    .any(|token| token.start as usize <= first && first < (token.end as usize));
                let trailing = content.trim_end_matches([' ', '\t', '\x0b']);
                if !string_line && trailing.len() != content.len() {
                    self.layout_diagnostic(
                        "C0303",
                        "trailing-whitespace",
                        "Trailing whitespace",
                        count as u32,
                        trailing.chars().count() as u32,
                        None,
                        offset + trailing.len(),
                    );
                }
            }
            if let Some(level) = indent.get(&(count as u32)) {
                self.check_indent_prefix(content, *level, count as u32, offset);
            }
            if logical {
                let current = if line.ends_with("\r\n") {
                    "\r\n"
                } else if line.ends_with('\n') {
                    "\n"
                } else if line.ends_with('\r') {
                    "\r"
                } else {
                    ""
                };
                if !current.is_empty() && ending.is_some_and(|prior| prior != current) {
                    self.layout_diagnostic(
                        "C0327",
                        "mixed-line-endings",
                        "Mixed line endings LF and CRLF",
                        count as u32,
                        0,
                        None,
                        offset,
                    );
                }
                ending = Some(current);
            }
            last_blank = content.is_empty();
        }
        let max = super::defaults::fixed().module_lines();
        if count > max {
            let number = super::directives::module_lines_pragma(&self.directives).unwrap_or(1);
            self.layout_diagnostic(
                "C0302",
                "too-many-lines",
                &format!("Too many lines in module ({count}/{max})"),
                number,
                0,
                None,
                0,
            );
        }
        if last_blank && count > 0 {
            let offset = lines(&source).last().map_or(0, |(offset, _)| *offset);
            self.layout_diagnostic(
                "C0305",
                "trailing-newlines",
                "Trailing newlines",
                count as u32,
                0,
                None,
                offset,
            );
        }
        if let Some(document) = document {
            self.check_note_tokens(document);
        }
    }
    fn check_indent_prefix(&mut self, line: &str, expected: usize, number: u32, offset: usize) {
        let indent = super::defaults::fixed().indent();
        let mut rest = line;
        let mut size = 0;
        while let Some(next) = rest.strip_prefix(indent) {
            size += indent.len();
            rest = next;
        }
        size += rest
            .chars()
            .take_while(|value| matches!(value, ' ' | '\t'))
            .count();
        let expected = expected * indent.len();
        if size != expected {
            self.layout_diagnostic(
                "W0311",
                "bad-indentation",
                &format!("Bad indentation. Found {size} spaces, expected {expected}"),
                number,
                0,
                None,
                offset,
            );
        }
    }
    fn check_note_tokens(&mut self, document: &SyntaxDocument<'_>) {
        for token in document
            .tokens
            .iter()
            .filter(|value| value.kind == "Comment")
        {
            let note = document
                .text(token)
                .strip_prefix('#')
                .unwrap_or_default()
                .trim_start_matches(whitespace);
            let matched = super::defaults::fixed().notes().iter().any(|value| {
                note_prefix_len(note, value).is_some_and(|prefix_len| {
                    note[prefix_len..]
                        .chars()
                        .next()
                        .is_none_or(|value| value == ':' || whitespace(value))
                })
            });
            if matched {
                let (line, column) = document.line_column(token.start);
                self.layout_diagnostic(
                    "W0511",
                    "fixme",
                    note,
                    line,
                    column,
                    None,
                    token.start as usize + 1,
                );
            }
        }
    }
}
fn note_prefix_len(note: &str, tag: &str) -> Option<usize> {
    let mut note_chars = note.char_indices();
    let mut end = 0;
    for expected in tag.chars() {
        let (index, actual) = note_chars.next()?;
        let ascii_match = actual.eq_ignore_ascii_case(&expected);
        let python_unicode_i_match = matches!(expected, 'i' | 'I') && matches!(actual, 'İ' | 'ı');
        if !ascii_match && !python_unicode_i_match {
            return None;
        }
        end = index + actual.len_utf8();
    }
    Some(end)
}
fn long_url() -> &'static regex::Regex {
    static VALUE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    VALUE.get_or_init(|| {
        regex::Regex::new(r"^\s*(# )?<?https?://\S+>?$").expect("locked long-line regex")
    })
}
fn line_length(line: &str) -> usize {
    let mut line = line;
    if let Some((value, _)) = line.split_once("# pylint:") {
        line = value.trim_end_matches(whitespace);
    }
    for marker in ["# type: ignore", "# noqa", "# pragma: no cover"] {
        if let Some((value, _)) = line.split_once(marker) {
            line = value.trim_end_matches(whitespace);
        }
    }
    line.chars().count()
}
