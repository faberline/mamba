//! Black keeps standalone comments on their side of the lambda colon.
use super::FormatBody;
use crate::comments::{SourceComment, dangling_comments, trailing_comments};
use crate::expression::parentheses::NeedsParentheses;
use crate::prelude::*;
use ruff_python_ast::{ExprLambda, token::TokenKind};
use ruff_text_size::{Ranged, TextRange};

pub(super) fn header(
    item: &ExprLambda,
    comments: &[SourceComment],
    f: &mut PyFormatter,
) -> FormatResult<bool> {
    if f.options().style_policy().is_none() || comments.is_empty() {
        return Ok(false);
    }
    let start = item.parameters.as_deref().map_or(item.start(), Ranged::end);
    let Some(colon) = f
        .context()
        .tokens()
        .in_range(TextRange::new(start, item.body.start()))
        .iter()
        .rev()
        .find(|token| token.kind() == TokenKind::Colon)
    else {
        return Ok(false);
    };
    let (before, after) =
        comments.split_at(comments.partition_point(|comment| comment.end() < colon.start()));
    if before
        .iter()
        .any(|comment| comment.line_position().is_own_line())
    {
        dangling_comments(before).fmt(f)?;
    }
    token(":").fmt(f)?;
    let after_own = after
        .iter()
        .any(|comment| comment.line_position().is_own_line());
    if after_own {
        trailing_comments(before).fmt(f)?;
        for comment in after {
            if comment.line_position().is_own_line() {
                dangling_comments(std::slice::from_ref(comment)).fmt(f)?;
            } else {
                trailing_comments(std::slice::from_ref(comment)).fmt(f)?;
            }
        }
    } else {
        space().fmt(f)?;
    }
    FormatBody {
        body: &item.body,
        dangling_header_comments: &[],
        needs_parentheses: item.body.needs_parentheses(item.into(), f.context()),
    }
    .fmt(f)?;
    if !after_own {
        trailing_comments(before).fmt(f)?;
        trailing_comments(after).fmt(f)?;
    }
    Ok(true)
}
