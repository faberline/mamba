//! Comments after a type-variable colon follow the bound's own brackets.
use crate::comments::trailing_comments;
use crate::expression::expr_tuple::TupleParentheses;
use crate::expression::parentheses::{Parentheses, parenthesized};
use crate::prelude::*;
use ruff_formatter::write;
use ruff_python_ast::Expr;
use ruff_text_size::Ranged;

pub(super) fn format(bound: &Expr, f: &mut PyFormatter) -> FormatResult<bool> {
    if f.options().style_policy().is_none() {
        return Ok(false);
    }
    let comments = f.context().comments().clone();
    let node_comments = comments.leading_dangling_trailing(bound);
    if !(node_comments.has_leading() || node_comments.has_trailing())
        || node_comments
            .into_iter()
            .any(|comment| comment.line_position().is_own_line())
    {
        return Ok(false);
    }
    let mut moved: Vec<_> = comments
        .leading(bound)
        .iter()
        .chain(comments.trailing(bound))
        .cloned()
        .collect();
    moved.sort_by_key(Ranged::start);
    for comment in comments
        .leading(bound)
        .iter()
        .chain(comments.trailing(bound))
    {
        comment.mark_formatted();
    }
    if let Expr::Subscript(subscript) = bound {
        let value = subscript.value.format().memoized();
        let slice = format_with(|f| {
            if let Expr::Tuple(tuple) = subscript.slice.as_ref() {
                tuple
                    .format()
                    .with_options(TupleParentheses::Preserve)
                    .fmt(f)
            } else {
                subscript.slice.format().fmt(f)
            }
        })
        .memoized();
        let flat = format_with(|f| {
            write!(f, [value, token("["), slice, token("]")])?;
            for comment in &moved {
                comment.mark_unformatted();
            }
            trailing_comments(&moved).fmt(f)
        });
        let expanded = format_with(|f| {
            for comment in &moved {
                comment.mark_unformatted();
            }
            write!(
                f,
                [
                    value,
                    parenthesized("[", &slice, "]").with_dangling_comments(&moved)
                ]
            )
        });
        best_fitting![flat, expanded]
            .with_mode(BestFittingMode::AllLines)
            .fmt(f)?;
    } else if let Expr::Tuple(tuple) = bound {
        let items = format_with(|f: &mut PyFormatter| {
            f.join_comma_separated(tuple.end())
                .nodes(&tuple.elts)
                .finish()
        })
        .memoized();
        let flat = format_with(|f| {
            parenthesized("(", &items, ")").fmt(f)?;
            for comment in &moved {
                comment.mark_unformatted();
            }
            trailing_comments(&moved).fmt(f)
        });
        let expanded = format_with(|f| {
            for comment in &moved {
                comment.mark_unformatted();
            }
            parenthesized("(", &items, ")")
                .with_dangling_comments(&moved)
                .fmt(f)
        });
        best_fitting![flat, expanded]
            .with_mode(BestFittingMode::AllLines)
            .fmt(f)?;
    } else {
        write!(f, [bound.format().with_options(Parentheses::Never)])?;
        for comment in &moved {
            comment.mark_unformatted();
        }
        trailing_comments(&moved).fmt(f)?;
    }
    Ok(true)
}
