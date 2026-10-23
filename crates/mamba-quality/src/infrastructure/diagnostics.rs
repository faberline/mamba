use crate::domain::{model::*, source::SourceSnapshot};
use ruff_source_file::LineIndex;
use ruff_text_size::{TextRange, TextSize};

pub fn at(
    source: &SourceSnapshot,
    tool: QualityTool,
    rule: &str,
    message: String,
    range: TextRange,
) -> QualityDiagnostic {
    let range = TextRange::new(
        TextSize::from(
            source
                .text
                .floor_char_boundary(range.start().to_usize().min(source.text.len()))
                as u32,
        ),
        TextSize::from(
            source
                .text
                .ceil_char_boundary(range.end().to_usize().min(source.text.len()))
                as u32,
        ),
    );
    let index = LineIndex::from_source_text(&source.text);
    let start = index.line_column(range.start(), &source.text);
    let end = index.line_column(range.end(), &source.text);
    let column_base = u32::from(tool != QualityTool::Pylint);
    let mut diagnostic = QualityDiagnostic {
        tool,
        rule: rule.into(),
        message,
        hint: None,
        path: source.path.clone(),
        severity: if tool == QualityTool::Pylint {
            match rule.as_bytes().first() {
                Some(b'C') => "convention",
                Some(b'W') => "warning",
                Some(b'R') => "refactor",
                Some(b'I') => "info",
                Some(b'F') => "fatal",
                _ => "error",
            }
        } else {
            "error"
        }
        .into(),
        location: QualityLocation {
            line: start.line.get() as u32,
            column: start.column.get() as u32 - 1 + column_base,
            end_line: Some(end.line.get() as u32),
            end_column: Some(i64::from(end.column.get() as u32 - 1 + column_base)),
            offset: Some(range.start().to_u32()),
            end_offset: Some(range.end().to_u32()),
        },
        fix: None,
    };
    if tool == QualityTool::Pylint
        && (!rule.starts_with("C03") || rule == "C0321")
        && !matches!(rule, "W1401" | "W1402")
    {
        // Astroid's syntax node columns use UTF-8 bytes. Text-only formatting
        // checks retain their separately computed character positions.
        diagnostic.location.column =
            range.start().to_u32() - index.line_start(start.line, &source.text).to_u32();
        diagnostic.location.end_column = Some(i64::from(
            range.end().to_u32() - index.line_start(end.line, &source.text).to_u32(),
        ));
    }
    if tool == QualityTool::Mypy {
        // mypy 2.4.0's pinned native parser converts Ruff's one-based columns again.
        let column = |location: ruff_source_file::LineColumn| {
            let line_start = index.line_start(location.line, &source.text).to_usize();
            let line = source.text[line_start..].split('\n').next().unwrap_or("");
            if line.is_ascii() {
                return location.column.get() as u32;
            }
            let target = source
                .text
                .floor_char_boundary((line_start + location.column.get()).min(source.text.len()));
            source.text[line_start..target].chars().count() as u32
        };
        diagnostic.location.column = column(start);
        diagnostic.location.end_column = Some(i64::from(column(end).saturating_sub(1)));
    }
    if range.is_empty() {
        diagnostic.location.end_line = None;
        diagnostic.location.end_column = None;
        diagnostic.location.end_offset = None;
    }
    diagnostic
}

pub fn point(offset: usize) -> TextRange {
    TextRange::empty(TextSize::from(offset as u32))
}
