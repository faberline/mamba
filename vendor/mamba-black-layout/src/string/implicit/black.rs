//! Black keeps contiguous literal runs together on each side of a comment.
use ruff_python_ast::StringLike;
use ruff_text_size::Ranged;

use crate::comments::{leading_comments, trailing_comments};
use crate::expression::parentheses::in_parentheses_only_soft_line_break_or_space;
use crate::prelude::*;

pub(super) fn format(string: StringLike<'_>, f: &mut PyFormatter) -> FormatResult<()> {
    let parts: Vec<_> = string.parts().collect();
    let comments = f.context().comments().clone();
    let mut start = 0;
    while start < parts.len() {
        let mut end = start + 1;
        while end < parts.len()
            && comments.trailing(parts[end - 1]).is_empty()
            && comments.leading(parts[end]).is_empty()
        {
            end += 1;
        }
        if start > 0 {
            in_parentheses_only_soft_line_break_or_space().fmt(f)?;
        }
        leading_comments(comments.leading(parts[start])).fmt(f)?;
        let trailing = comments.trailing(parts[end - 1]);
        let own_line = trailing.partition_point(|comment| comment.line_position().is_end_of_line());
        for comment in &trailing[own_line..] {
            comment.mark_formatted();
        }
        group(&format_with(|f: &mut PyFormatter| {
            let mut joiner = f.join_with(in_parentheses_only_soft_line_break_or_space());
            for (index, part) in parts[start..end].iter().enumerate() {
                let contents = format_with(|f| match part {
                    ruff_python_ast::StringLikePart::String(value) => value.format().fmt(f),
                    ruff_python_ast::StringLikePart::Bytes(value) => value.format().fmt(f),
                    ruff_python_ast::StringLikePart::FString(value)
                        if start + index + 1 < parts.len()
                            && crate::other::f_string::has_skipped_interpolation_comment(
                                &value.elements,
                                f,
                                f.context().source(),
                            ) =>
                    {
                        let source = f.context().source();
                        let backslash = value
                            .elements
                            .interpolations()
                            .any(|element| source[element.range()].contains('\\'));
                        let normalized = f
                            .options()
                            .style_policy()
                            .expect("Black string formatting requires a style policy")
                            .interpolated(&source[value.range()], backslash);
                        let candidate =
                            crate::other::f_string::black_skipped_interpolation_candidate(
                                &value.elements,
                                f,
                                &normalized,
                            )
                            .unwrap_or(normalized);
                        f.context()
                            .comments()
                            .mark_verbatim_node_comments_formatted((*value).into());
                        text(&candidate).fmt(f)
                    }
                    ruff_python_ast::StringLikePart::FString(value) => value.format().fmt(f),
                    ruff_python_ast::StringLikePart::TString(value)
                        if start + index + 1 < parts.len()
                            && crate::other::f_string::has_skipped_interpolation_comment(
                                &value.elements,
                                f,
                                f.context().source(),
                            ) =>
                    {
                        let source = f.context().source();
                        let backslash = value
                            .elements
                            .interpolations()
                            .any(|element| source[element.range()].contains('\\'));
                        let normalized = f
                            .options()
                            .style_policy()
                            .expect("Black string formatting requires a style policy")
                            .interpolated(&source[value.range()], backslash);
                        let candidate =
                            crate::other::f_string::black_skipped_interpolation_candidate(
                                &value.elements,
                                f,
                                &normalized,
                            )
                            .unwrap_or(normalized);
                        f.context()
                            .comments()
                            .mark_verbatim_node_comments_formatted((*value).into());
                        text(&candidate).fmt(f)
                    }
                    ruff_python_ast::StringLikePart::TString(value) => value.format().fmt(f),
                });
                joiner.entry(&contents);
            }
            joiner.finish()
        }))
        .should_expand(f.context().bracket_is_forced(string.range()))
        .fmt(f)?;
        for comment in &trailing[own_line..] {
            comment.mark_unformatted();
        }
        trailing_comments(trailing).fmt(f)?;
        if trailing
            .iter()
            .any(|comment| comment.line_position().is_end_of_line())
        {
            expand_parent().fmt(f)?;
        }
        start = end;
    }
    Ok(())
}
