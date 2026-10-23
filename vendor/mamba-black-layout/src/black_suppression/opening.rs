//! An opening-bracket skip can cover the whole enclosing simple statement.
use crate::prelude::*;
use ruff_python_ast::{AnyNodeRef, token::TokenKind};
use ruff_python_trivia::SuppressionKind;
use ruff_text_size::{Ranged, TextLen, TextRange};

pub(crate) fn first_comment_skips(range: TextRange, context: &PyFormatContext) -> bool {
    let mut previous = None;
    let mut comment_seen = false;
    for token in context.tokens().in_range(range).iter() {
        match token.kind() {
            TokenKind::Comment => {
                if !comment_seen
                    && previous.is_some_and(|opening: ruff_python_ast::token::Token| {
                        matches!(
                            opening.kind(),
                            TokenKind::Lpar | TokenKind::Lsqb | TokenKind::Lbrace
                        ) && !context.source()[TextRange::new(range.start(), opening.start())]
                            .contains(['\r', '\n'])
                    })
                    && crate::comments::suppression::controls(&context.source()[token.range()])
                        .contains(&SuppressionKind::Skip)
                {
                    return true;
                }
                comment_seen = true;
            }
            kind if kind.is_trivia()
                || kind.is_any_newline()
                || matches!(kind, TokenKind::Indent | TokenKind::Dedent) => {}
            _ => {
                previous = Some(*token);
                comment_seen = false;
            }
        }
    }
    false
}

pub(crate) fn format_fields(node: AnyNodeRef, f: &mut PyFormatter) -> FormatResult<bool> {
    if f.options().style_policy().is_none()
        || !(node.is_stmt_expr()
            || node.is_stmt_assign()
            || node.is_stmt_ann_assign()
            || node.is_stmt_aug_assign()
            || node.is_stmt_import_from()
            || node.is_stmt_return()
            || node.is_stmt_raise()
            || node.is_stmt_assert()
            || node.is_stmt_delete())
        || !first_comment_skips(node.range(), f.context())
    {
        return Ok(false);
    }
    let suffix = f
        .context()
        .tokens()
        .in_range(TextRange::new(node.end(), f.context().source().text_len()));
    let end = suffix
        .iter()
        .take_while(|token| token.kind() == TokenKind::Comment)
        .last()
        .map_or(node.end(), Ranged::end);
    let range = TextRange::new(node.start(), end);
    super::line::mark(node, range, f.context().comments());
    crate::verbatim::verbatim_text(range).fmt(f)?;
    Ok(true)
}
