//! Type-comment line boundaries from Black's stable style.
use ruff_formatter::FormatContext;
use ruff_python_ast::{AnyNodeRef, token::TokenKind};
use ruff_text_size::{Ranged, TextLen, TextRange};

use crate::{PyFormatContext, comments::SourceComment};

/// A continuation after real code keeps the next comment on that logical line.
pub(crate) fn follows_escaped_code(
    start: ruff_text_size::TextSize,
    source: &str,
    trivia: &ruff_python_trivia::TriviaRanges,
) -> bool {
    let prefix = &source[..usize::from(start)];
    let Some((earlier, current)) = prefix.rsplit_once('\n') else {
        return false;
    };
    if !current.chars().all(|character| character.is_whitespace()) {
        return false;
    }
    let preceding = earlier.rsplit_once('\n').map_or(earlier, |(_, line)| line);
    if !preceding
        .strip_suffix('\\')
        .is_some_and(|code| !code.trim().is_empty())
    {
        return false;
    }
    let end = ruff_text_size::TextSize::try_from(earlier.len()).unwrap();
    !trivia.comments().intersects(TextRange::new(
        end - ruff_text_size::TextSize::from(1u32),
        end,
    ))
}

/// Count physical blank lines and continuation-only lines before a block item.
pub(crate) fn block_lines_before(start: ruff_text_size::TextSize, source: &str) -> u32 {
    source[..usize::from(start)]
        .chars()
        .rev()
        .take_while(|character| character.is_whitespace() || *character == '\\')
        .filter(|character| *character == '\n')
        .count()
        .try_into()
        .unwrap_or(u32::MAX)
}

pub(crate) fn type_payload(source: &str) -> Option<&str> {
    source
        .strip_prefix('#')?
        .trim_start_matches(|c: char| c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c))
        .strip_prefix("type:")
}

pub(crate) fn is_ignore(source: &str) -> bool {
    type_payload(source).is_some_and(|value| value.trim_start().starts_with("ignore"))
}

/// A source header that ends in a type ignore must remain one physical line.
pub(crate) fn unsplittable_header_ignore(
    comment: &SourceComment,
    context: &PyFormatContext<'_>,
) -> bool {
    if !comment.line_position().is_end_of_line() || !is_ignore(&context.source()[comment.range()]) {
        return false;
    }
    let first = context
        .tokens()
        .in_range(TextRange::new(0.into(), comment.start()))
        .iter()
        .rev()
        .take_while(|token| token.kind() != TokenKind::Newline)
        .filter(|token| {
            !token.kind().is_trivia()
                && !matches!(token.kind(), TokenKind::Indent | TokenKind::Dedent)
        })
        .last();
    first.is_some_and(|first| {
        !context.source()[TextRange::new(first.start(), comment.start())].contains(['\r', '\n'])
    })
}

/// Collapsing a later type comment behind this comment would hide its meaning.
pub(crate) fn has_later_type_comment(
    comment: &SourceComment,
    context: &PyFormatContext<'_>,
) -> bool {
    context
        .tokens()
        .in_range(TextRange::new(comment.end(), context.source().text_len()))
        .iter()
        .take_while(|token| token.kind() != TokenKind::Newline)
        .any(|token| {
            token.kind() == TokenKind::Comment
                && type_payload(&context.source()[token.range()]).is_some()
        })
}

pub(crate) fn has_earlier_type_comment(
    comment: &SourceComment,
    context: &PyFormatContext<'_>,
) -> bool {
    context
        .tokens()
        .in_range(TextRange::new(0.into(), comment.start()))
        .iter()
        .rev()
        .take_while(|token| token.kind() != TokenKind::Newline)
        .any(|token| {
            token.kind() == TokenKind::Comment
                && type_payload(&context.source()[token.range()]).is_some()
        })
}

pub(crate) fn multiple_type_ignores(node: AnyNodeRef<'_>, context: &PyFormatContext<'_>) -> bool {
    let within = context
        .tokens()
        .in_range(node.range())
        .iter()
        .filter(|token| {
            token.kind() == TokenKind::Comment && is_ignore(&context.source()[token.range()])
        })
        .count();
    let outside = context
        .comments()
        .leading(node)
        .iter()
        .chain(context.comments().trailing(node))
        .filter(|comment| {
            !node.range().contains_range(comment.range())
                && is_ignore(&context.source()[comment.range()])
        })
        .count();
    within + outside > 1
}

/// A type ignore on one physical line must not be distributed over new lines.
pub(crate) fn unsplittable(node: AnyNodeRef<'_>, context: &PyFormatContext<'_>) -> bool {
    context.options().style_policy().is_some()
        && (node.is_statement() || node.is_expression() || node.is_keyword())
        && !crate::expression::black_trailers::has_breaking_magic(node, context)
        && !context.source()[node.range()].contains(['\r', '\n'])
        && context.comments().trailing(node).iter().any(|comment| {
            comment.line_position().is_end_of_line()
                && comment.start() >= node.end()
                && is_ignore(&context.source()[comment.range()])
                && !context.source()[TextRange::new(node.end(), comment.start())]
                    .contains(['\r', '\n'])
        })
}
