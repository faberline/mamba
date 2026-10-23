//! Black's boundaries between code and comment-only suppression blocks.
use crate::PyFormatContext;
use crate::comments::SourceComment;
use ruff_formatter::FormatContext;
use ruff_python_ast::token::TokenKind;
use ruff_python_trivia::SuppressionKind;
use ruff_text_size::{Ranged, TextLen, TextRange};

pub(crate) fn definition_after_block(
    position: ruff_text_size::TextSize,
    context: &PyFormatContext,
) -> bool {
    let Some(end) = context.off_ranges().end_at(position) else {
        return false;
    };
    let mut on_end = None;
    for token in context
        .tokens()
        .in_range(TextRange::new(end, context.source().text_len()))
        .iter()
    {
        if token.kind() == TokenKind::Comment {
            if crate::comments::suppression::controls(&context.source()[token.range()])
                .contains(&SuppressionKind::On)
            {
                on_end = Some(token.end());
            }
        } else if code(token.kind()) {
            return matches!(
                token.kind(),
                TokenKind::Def | TokenKind::Class | TokenKind::At
            ) && on_end.is_some_and(|end| {
                context.source()[TextRange::new(end, token.start())]
                    .bytes()
                    .filter(|byte| *byte == b'\n')
                    .count()
                    <= 1
            });
        }
    }
    false
}

fn code(kind: TokenKind) -> bool {
    !kind.is_trivia()
        && !kind.is_any_newline()
        && !matches!(
            kind,
            TokenKind::Indent | TokenKind::Dedent | TokenKind::EndOfFile
        )
}

pub(crate) fn replace_on_with_blank(
    off: &SourceComment,
    on: &SourceComment,
    next_off: Option<&SourceComment>,
    context: &PyFormatContext,
) -> bool {
    let Some(next_off) = next_off else {
        return false;
    };
    if context.options().style_policy().is_none() {
        return false;
    }
    let mut code_seen = false;
    for token in context
        .tokens()
        .in_range(TextRange::new(off.end(), on.start()))
        .iter()
    {
        if token.kind() == TokenKind::Comment
            && crate::comments::suppression::controls(&context.source()[token.range()])
                .contains(&SuppressionKind::Off)
        {
            code_seen = false;
        } else if code(token.kind()) {
            code_seen = true;
        }
    }
    if !code_seen {
        return false;
    }
    for token in context
        .tokens()
        .in_range(TextRange::new(next_off.end(), context.source().text_len()))
        .iter()
    {
        if code(token.kind()) {
            return false;
        }
        if token.kind() == TokenKind::Comment
            && crate::comments::suppression::controls(&context.source()[token.range()])
                .contains(&SuppressionKind::On)
        {
            return true;
        }
    }
    false
}

/// A later comment-only block in a with-header prefix starts a new comment group.
pub(crate) fn empty_with_gap(on: &SourceComment, context: &PyFormatContext) -> bool {
    if context.options().style_policy().is_none() || !on.is_suppression_on_comment(context.source())
    {
        return false;
    }
    let preceding = context
        .tokens()
        .in_range(TextRange::new(0.into(), on.start()));
    let Some(off) = preceding
        .iter()
        .rev()
        .find(|token| !token.kind().is_any_newline())
    else {
        return false;
    };
    if off.kind() != TokenKind::Comment
        || !crate::comments::suppression::controls(&context.source()[off.range()])
            .contains(&SuppressionKind::Off)
    {
        return false;
    }
    let mut ordinary = false;
    let mut later_block = false;
    let mut ended = false;
    for token in context
        .tokens()
        .in_range(TextRange::new(on.end(), context.source().text_len()))
        .iter()
    {
        if token.kind() == TokenKind::Comment {
            let controls = crate::comments::suppression::controls(&context.source()[token.range()]);
            if controls.contains(&SuppressionKind::Off) {
                later_block = ordinary;
            } else if controls.contains(&SuppressionKind::On) {
                ended = later_block;
            } else {
                ordinary = true;
            }
        } else if code(token.kind()) {
            return ended && token.kind() == TokenKind::With;
        }
    }
    false
}
