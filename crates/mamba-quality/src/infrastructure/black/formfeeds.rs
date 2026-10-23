//! Restore Black's top-level form-feed prefixes through exact source markers.
use mamba_language::application::quality_syntax::{self, PythonTarget};
use ruff_formatter::SourceMarker;
use ruff_python_ast::token::TokenKind;
use ruff_text_size::Ranged;
use std::collections::{BTreeMap, BTreeSet};

fn whitespace(character: char) -> bool {
    matches!(character, ' ' | '\t' | '\r' | '\n' | '\x0c')
}

fn prefix(text: &str, end: usize) -> &str {
    let start = text[..end]
        .char_indices()
        .rev()
        .find(|(_, character)| !whitespace(*character) && *character != '\\')
        .map_or(0, |(start, character)| start + character.len_utf8());
    &text[start..end]
}

pub(super) fn restore(
    source: &str,
    output: &str,
    markers: &[SourceMarker],
    target: PythonTarget,
    is_stub: bool,
) -> String {
    let Ok(document) = quality_syntax::parse(source, target, is_stub) else {
        return output.into();
    };
    let mut anchors = BTreeSet::new();
    let mut brackets = 0u32;
    let mut interpolation = 0u32;
    for token in document.parsed.tokens().iter() {
        match token.kind() {
            TokenKind::FStringStart | TokenKind::TStringStart => {
                if interpolation == 0 && brackets == 0 {
                    anchors.insert(token.start());
                }
                interpolation += 1;
            }
            TokenKind::FStringEnd | TokenKind::TStringEnd => {
                interpolation = interpolation.saturating_sub(1)
            }
            _ if interpolation > 0 => {}
            TokenKind::Lpar | TokenKind::Lsqb | TokenKind::Lbrace => {
                if brackets == 0 {
                    anchors.insert(token.start());
                }
                brackets += 1;
            }
            TokenKind::Rpar | TokenKind::Rsqb | TokenKind::Rbrace => {
                brackets = brackets.saturating_sub(1)
            }
            kind if brackets == 0
                && !kind.is_any_newline()
                && !matches!(
                    kind,
                    TokenKind::Indent | TokenKind::Dedent | TokenKind::EndOfFile
                ) =>
            {
                anchors.insert(token.start());
            }
            _ => {}
        }
    }
    let mut edits = BTreeMap::new();
    for marker in markers {
        let start = usize::from(marker.source);
        let dest = usize::from(marker.dest);
        if !anchors.contains(&marker.source) || dest == 0 || dest >= output.len() {
            continue;
        }
        let input_prefix = prefix(source, start);
        let prefix_start = start - input_prefix.len();
        let first_full_line = prefix_start == 0 || source[..prefix_start].ends_with('\n');
        let has_feed = input_prefix
            .split_inclusive('\n')
            .enumerate()
            .any(|(index, line)| {
                (index > 0 || first_full_line)
                    && line.ends_with('\n')
                    && line.contains('\x0c')
                    && !line.contains('\\')
            });
        let escaped_feed = input_prefix
            .split_inclusive('\n')
            .enumerate()
            .any(|(index, line)| {
                (index > 0 || first_full_line)
                    && line.strip_suffix("\\\n").is_some_and(|prefix| {
                        prefix.contains('\x0c') && prefix.chars().all(whitespace)
                    })
            });
        let line_start = output[..dest].rfind('\n').map_or(0, |newline| newline + 1);
        if !(has_feed || escaped_feed) || line_start != dest || output[..dest].trim().is_empty() {
            continue;
        }
        let output_prefix = prefix(output, dest);
        if output_prefix.contains('\x0c') {
            continue;
        }
        let replace_start = if has_feed && output_prefix.ends_with("\n\n") {
            dest - 1
        } else {
            dest
        };
        let replacement = if has_feed {
            "\x0c\n"
        } else {
            let maximum = if is_stub { 1 } else { 2 };
            if output_prefix
                .bytes()
                .filter(|byte| *byte == b'\n')
                .count()
                .saturating_sub(1)
                >= maximum
            {
                continue;
            }
            "\n"
        };
        edits.insert(replace_start, (dest, replacement));
    }
    let mut result = output.to_owned();
    for (start, (end, replacement)) in edits.into_iter().rev() {
        result.replace_range(start..end, replacement);
    }
    result
}
