use ruff_formatter::prelude::{space, token};
use ruff_formatter::{FormatOptions, write};
use ruff_python_ast::StmtAssert;
use ruff_text_size::Ranged;

use crate::expression::maybe_parenthesize_expression;
use crate::expression::parentheses::Parenthesize;
use crate::expression::parentheses::{NeedsParentheses, OptionalParentheses};
use crate::prelude::*;
mod black;

#[derive(Default)]
pub struct FormatStmtAssert;

impl FormatNodeRule<StmtAssert> for FormatStmtAssert {
    fn fmt_fields(&self, item: &StmtAssert, f: &mut PyFormatter) -> FormatResult<()> {
        let StmtAssert {
            range: _,
            node_index: _,
            test,
            msg,
        } = item;

        if let Some(message) = msg
            && f.options().style_policy().is_some()
            && !f
                .context()
                .comments()
                .contains_comments(test.as_ref().into())
            && !f
                .context()
                .comments()
                .contains_comments(message.as_ref().into())
            && !f.context().comments().has_dangling(item)
            && !f.context().comments().has_trailing(item)
        {
            if black::own_split(item, f)? {
                return Ok(());
            }
            write!(f, [token("assert"), space()])?;
            let short_atomic_test = (test.is_name_expr()
                || test.is_boolean_literal_expr()
                || test.is_none_literal_expr()
                || test.is_number_literal_expr()
                || test.is_ellipsis_literal_expr())
                && ruff_formatter::black_width::measure(&f.context().source()[test.range()])
                    + f.context()
                        .indent_level()
                        .to_ascii_spaces(f.options().indent_width()) as usize
                    + 10
                    <= usize::from(f.options().line_width().value());
            if short_atomic_test
                && message.needs_parentheses(item.into(), f.context())
                    == OptionalParentheses::Multiline
            {
                maybe_parenthesize_expression(test, item, Parenthesize::IfRequired).fmt(f)?;
                write!(f, [token(","), space()])?;
                return maybe_parenthesize_expression(message, item, Parenthesize::IfBreaks).fmt(f);
            }
            // With no comment boundary, Black tries the test's split points
            // before introducing optional parentheses around the message.
            crate::statement::stmt_assign::FormatStatementsLastExpression::left_to_right(
                test, item,
            )
            .fmt(f)?;
            write!(f, [token(","), space()])?;
            let available = usize::from(f.options().line_width().value()).saturating_sub(
                f.context()
                    .indent_level()
                    .to_ascii_spaces(f.options().indent_width()) as usize
                    + 4,
            );
            let raw_message = &f.context().source()[message.range()];
            let flat_fallback = test.is_name_expr()
                && ruff_formatter::black_width::measure(&f.context().source()[test.range()])
                    > available
                && message
                    .as_string_literal_expr()
                    .is_some_and(|string| !string.value.is_implicit_concatenated())
                && !raw_message.contains(['\r', '\n'])
                && ruff_formatter::black_width::measure(
                    &f.options().style_policy().unwrap().literal(raw_message),
                ) <= available;
            if flat_fallback {
                token("(").fmt(f)?;
            }
            let message_parentheses = if message.needs_parentheses(item.into(), f.context())
                == OptionalParentheses::Multiline
            {
                Parenthesize::IfBreaks
            } else {
                Parenthesize::IfRequired
            };
            maybe_parenthesize_expression(message, item, message_parentheses).fmt(f)?;
            if flat_fallback {
                token(")").fmt(f)?;
            }
            return Ok(());
        }

        write!(
            f,
            [
                token("assert"),
                space(),
                maybe_parenthesize_expression(test, item, Parenthesize::IfBreaks)
            ]
        )?;

        if let Some(msg) = msg {
            write!(
                f,
                [
                    token(","),
                    space(),
                    maybe_parenthesize_expression(msg, item, Parenthesize::IfBreaksParenthesized),
                ]
            )?;
        }

        Ok(())
    }
}
