//! Stable Black splits a dictionary key's operators before a value's split points.
use crate::comments::{SourceComment, trailing_comments};
use crate::expression::binary_like::BinaryLike;
use crate::expression::parentheses::Parentheses;
use crate::prelude::*;
use ruff_formatter::{RemoveSoftLinesBuffer, write};
use ruff_python_ast::Expr;
use ruff_python_ast::token::parentheses_iterator;
use ruff_text_size::Ranged;

pub(super) fn format_key(key: &Expr, f: &mut PyFormatter) -> FormatResult<()> {
    if f.options().style_policy().is_some() {
        let layers = parentheses_iterator(key.into(), None, f.context().tokens()).count();
        if layers > 1 && !f.context().comments().contains_comments(key.into()) {
            return crate::expression::black_parentheses::format_layers(
                &key.format().with_options(Parentheses::Never),
                layers,
                f,
            );
        }
    }
    key.format().fmt(f)
}

pub(super) fn format_pair(
    key: &Expr,
    value: &Expr,
    comments: &[SourceComment],
    f: &mut PyFormatter,
) -> FormatResult<bool> {
    if f.options().style_policy().is_none()
        || !key.is_bin_op_expr()
        || f.context().is_expression_parenthesized(key.into())
        || f.context().comments().contains_comments(key.into())
        || crate::comments::has_skip_comment(comments, f.context().source())
        || comments.iter().any(|comment| {
            comment.line_position().is_own_line()
                || crate::black_comments::type_payload(&f.context().source()[comment.range()])
                    .is_some()
                || comment.is_suppression_on_comment(f.context().source())
                || comment.is_suppression_off_comment(f.context().source())
        })
        || crate::black_multiline::contains(value, f.context().source())
    {
        return Ok(false);
    }
    for comment in comments {
        comment.mark_formatted();
    }
    let preserve_value_lines = f.context().comments().has_leading(value)
        || f.context().comments().has_trailing_own_line(value);
    let binary = key.as_bin_op_expr().unwrap();
    let expanded_key = format_with(|f| BinaryLike::Binary(binary).fmt_expanded(f)).memoized();
    let key = key.format().memoized();
    let value = format_with(|f| {
        value.format().fmt(f)?;
        for comment in comments {
            comment.mark_unformatted();
            trailing_comments(std::slice::from_ref(comment)).fmt(f)?;
        }
        Ok(())
    })
    .memoized();
    let flat = format_with(|f| {
        let mut buffer = RemoveSoftLinesBuffer::new(f);
        write!(buffer, [key, token(":"), space(), value])
    });
    let split_key = format_with(|f| {
        write!(f, [expanded_key, token(":"), space()])?;
        let mut buffer = RemoveSoftLinesBuffer::new(f);
        write!(buffer, [value])
    });
    let expanded = format_with(|f| write!(f, [expanded_key, token(":"), space(), value]));
    if preserve_value_lines {
        expanded.fmt(f)?;
        return Ok(true);
    }
    best_fitting![flat, split_key, expanded]
        .with_mode(BestFittingMode::AllLines)
        .fmt(f)?;
    Ok(true)
}
