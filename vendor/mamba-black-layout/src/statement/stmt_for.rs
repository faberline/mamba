use ruff_formatter::{RemoveSoftLinesBuffer, format_args, write};
use ruff_python_ast::{Expr, Stmt, StmtFor};
use ruff_text_size::Ranged;

use crate::expression::expr_tuple::TupleParentheses;
use crate::expression::maybe_parenthesize_expression;
use crate::expression::parentheses::{Parentheses, Parenthesize};
use crate::prelude::*;
use crate::statement::clause::{ClauseHeader, ElseClause, clause};
use crate::statement::suite::SuiteKind;

#[derive(Debug)]
struct ExprTupleWithoutParentheses<'a>(&'a Expr);

impl Format<PyFormatContext<'_>> for ExprTupleWithoutParentheses<'_> {
    fn fmt(&self, f: &mut PyFormatter) -> FormatResult<()> {
        match self.0 {
            Expr::Named(_) if f.options().style_policy().is_some() => {
                self.0.format().with_options(Parentheses::Always).fmt(f)
            }
            Expr::Tuple(tuple)
                if f.options().style_policy().is_some()
                    && tuple.elts.iter().any(Expr::is_named_expr) =>
            {
                tuple.format().with_options(TupleParentheses::Always).fmt(f)
            }
            Expr::Tuple(expr_tuple) => expr_tuple
                .format()
                .with_options(TupleParentheses::NeverPreserve)
                .fmt(f),
            other => maybe_parenthesize_expression(other, self.0, Parenthesize::IfBreaks).fmt(f),
        }
    }
}

struct FormatForHeader<'a>(&'a StmtFor);

impl Format<PyFormatContext<'_>> for FormatForHeader<'_> {
    fn fmt(&self, f: &mut PyFormatter) -> FormatResult<()> {
        let item = self.0;
        let normal = format_with(|f| {
            write!(
                f,
                [
                    item.is_async
                        .then_some(format_args![token("async"), space()]),
                    token("for"),
                    space(),
                    ExprTupleWithoutParentheses(&item.target),
                    space(),
                    token("in"),
                    space(),
                    maybe_parenthesize_expression(&item.iter, item, Parenthesize::IfBreaks),
                ]
            )
        });
        let Some(tuple) = item.target.as_tuple_expr().filter(|tuple| tuple.len() > 1) else {
            return normal.fmt(f);
        };
        if f.options().style_policy().is_none()
            || f.context()
                .comments()
                .contains_comments(item.target.as_ref().into())
            || f.context()
                .comments()
                .contains_comments(item.iter.as_ref().into())
            || f.context().comments().has_dangling(item)
        {
            return normal.fmt(f);
        }
        let flat = format_with(|f| {
            let mut buffer = RemoveSoftLinesBuffer::new(f);
            write!(buffer, [normal])
        });
        // Black splits the tuple target first when the complete header is wide.
        // The iterator can then use its own split points after the target closes.
        let expanded = format_with(|f: &mut PyFormatter| {
            write!(
                f,
                [
                    item.is_async
                        .then_some(format_args![token("async"), space()]),
                    token("for"),
                    space(),
                    token("("),
                    block_indent(&format_with(|f: &mut PyFormatter| {
                        f.join_with(&format_args![token(","), hard_line_break()])
                            .entries(tuple.elts.iter().formatted())
                            .finish()?;
                        token(",").fmt(f)
                    })),
                    token(")"),
                    space(),
                    token("in"),
                    space(),
                    maybe_parenthesize_expression(&item.iter, item, Parenthesize::IfBreaks),
                ]
            )
        });
        if crate::other::commas::has_magic_trailing_comma(
            ruff_text_size::TextRange::new(tuple.elts.last().unwrap().end(), tuple.end()),
            f.context(),
        ) {
            expanded.fmt(f)
        } else {
            best_fitting![flat, expanded]
                .with_mode(BestFittingMode::AllLines)
                .fmt(f)
        }
    }
}

#[derive(Default)]
pub struct FormatStmtFor;

impl FormatNodeRule<StmtFor> for FormatStmtFor {
    fn fmt_fields(&self, item: &StmtFor, f: &mut PyFormatter) -> FormatResult<()> {
        let StmtFor {
            is_async: _,
            target: _,
            iter,
            body,
            orelse,
            range: _,
            node_index: _,
        } = item;

        let comments = f.context().comments().clone();
        let dangling_comments = comments.dangling(item);
        let body_start = body.first().map_or(iter.end(), Stmt::start);
        let or_else_comments_start =
            dangling_comments.partition_point(|comment| comment.end() < body_start);

        let (trailing_condition_comments, or_else_comments) =
            dangling_comments.split_at(or_else_comments_start);

        write!(
            f,
            [clause(
                ClauseHeader::For(item),
                &FormatForHeader(item),
                trailing_condition_comments,
                body,
                SuiteKind::other(orelse.is_empty()),
            ),]
        )?;

        if orelse.is_empty() {
            debug_assert!(or_else_comments.is_empty());
        } else {
            // Split between leading comments before the `else` keyword and end of line comments at the end of
            // the `else:` line.
            let trailing_start =
                or_else_comments.partition_point(|comment| comment.line_position().is_own_line());
            let (leading, trailing) = or_else_comments.split_at(trailing_start);

            write!(
                f,
                [clause(
                    ClauseHeader::OrElse(ElseClause::For(item)),
                    &token("else"),
                    trailing,
                    orelse,
                    SuiteKind::other(true),
                )
                .with_leading_comments(leading, body.last())]
            )?;
        }

        Ok(())
    }
}
