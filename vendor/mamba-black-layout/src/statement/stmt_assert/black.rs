//! Try the test's real brackets before introducing optional test parentheses.
use crate::black_measurement as measurement;
use crate::expression::black_trailers::{has_magic, last_chain_bracket};
use crate::expression::maybe_parenthesize_expression;
use crate::expression::parentheses::{
    NeedsParentheses, OptionalParentheses, Parentheses, Parenthesize, parenthesized,
};
use crate::prelude::*;
use crate::statement::stmt_assign::FormatStatementsLastExpression;
use ruff_formatter::{format_args, write};
use ruff_python_ast::{
    Expr, StmtAssert,
    visitor::{self, Visitor},
};

#[derive(Default)]
struct Split {
    found: bool,
}

impl<'a> Visitor<'a> for Split {
    fn visit_expr(&mut self, item: &'a Expr) {
        self.found |= match item {
            Expr::Call(call) => !call.arguments.is_empty(),
            Expr::Subscript(_) => true,
            Expr::Tuple(tuple) => !tuple.elts.is_empty(),
            Expr::List(list) => !list.elts.is_empty(),
            Expr::Set(set) => !set.elts.is_empty(),
            Expr::Dict(dict) => !dict.items.is_empty(),
            _ => false,
        };
        if !self.found {
            visitor::walk_expr(self, item);
        }
    }
}

pub(super) fn own_split(item: &StmtAssert, f: &mut PyFormatter) -> FormatResult<bool> {
    let mut split = Split::default();
    split.visit_expr(&item.test);
    split.found |= item.test.as_bool_op_expr().is_some_and(|boolean| {
        boolean
            .values
            .iter()
            .any(|value| f.context().is_expression_parenthesized(value.into()))
    });
    if !split.found {
        return Ok(false);
    }
    let message = item.msg.as_deref().unwrap();
    let test = maybe_parenthesize_expression(&item.test, item, Parenthesize::IfRequired).memoized();
    let plain_message =
        maybe_parenthesize_expression(message, item, Parenthesize::IfRequired).memoized();
    let plain = format_with(|f| {
        group(&format_args![
            token("assert"),
            space(),
            test,
            token(","),
            space(),
            plain_message
        ])
        .should_expand(true)
        .fmt(f)
    })
    .memoized();
    if has_magic(&item.test, f.context())
        && message.needs_parentheses(item.into(), f.context()) == OptionalParentheses::Multiline
    {
        write!(f, [token("assert"), space(), test, token(","), space()])?;
        parenthesized("(", &message.format().with_options(Parentheses::Never), ")")
            .with_force_expansion(true)
            .fmt(f)?;
        return Ok(true);
    }
    if comparison_needs_outer(&item.test, f.context())
        && !measurement::flat_line_fits(plain.inspect(f)?, f.context())?
    {
        return comparison_split(item, f);
    }
    if f.context().forced_bracket().is_none()
        && message
            .as_string_literal_expr()
            .is_some_and(|string| !string.value.is_implicit_concatenated())
        && !f
            .context()
            .is_expression_parenthesized(item.test.as_ref().into())
        && !has_magic(&item.test, f.context())
        && !measurement::flat_line_fits(plain.inspect(f)?, f.context())?
        && let Some(range) = last_chain_bracket(&item.test, f.context())
    {
        return crate::context::with_forced_bracket(range, f, |f| own_split(item, f));
    }
    let head_fits = measurement::first_line_fits(plain.inspect(f)?, f.context())?;
    let message_split = format_with(|f| {
        group(&format_args![
            token("assert"),
            space(),
            test,
            token(","),
            space(),
            FormatStatementsLastExpression::left_to_right(message, item)
        ])
        .should_expand(true)
        .fmt(f)
    });
    let last = format_with(|f: &mut PyFormatter| {
        if head_fits {
            return plain.fmt(f);
        }
        write!(
            f,
            [
                token("assert"),
                space(),
                FormatStatementsLastExpression::left_to_right(&item.test, item),
                token(","),
                space(),
            ]
        )?;
        let parentheses = if message.needs_parentheses(item.into(), f.context())
            == OptionalParentheses::Multiline
        {
            Parenthesize::IfBreaks
        } else {
            Parenthesize::IfRequired
        };
        maybe_parenthesize_expression(message, item, parentheses).fmt(f)
    });
    best_fitting![plain, message_split, last]
        .with_mode(BestFittingMode::AllLines)
        .fmt(f)?;
    Ok(true)
}

fn comparison_needs_outer(test: &Expr, context: &PyFormatContext) -> bool {
    let Some(compare) = test.as_compare_expr() else {
        return false;
    };
    let first = compare.first_operand();
    let last = compare.operands.last().unwrap();
    !context.is_expression_parenthesized(first.into())
        && !context.is_expression_parenthesized(last.into())
        && !matches!(
            first,
            Expr::Tuple(_) | Expr::List(_) | Expr::Set(_) | Expr::Dict(_)
        )
        && !matches!(
            last,
            Expr::Call(_) | Expr::Tuple(_) | Expr::List(_) | Expr::Set(_) | Expr::Dict(_)
        )
}

fn comparison_split(item: &StmtAssert, f: &mut PyFormatter) -> FormatResult<bool> {
    let message = item.msg.as_deref().unwrap();
    let multiline =
        message.needs_parentheses(item.into(), f.context()) == OptionalParentheses::Multiline;
    let flat_test = format_with(|f| {
        let mut flat = ruff_formatter::RemoveSoftLinesBuffer::new(f);
        write!(flat, [item.test.format().with_options(Parentheses::Never)])
    })
    .memoized();
    let message_split = format_with(|f| {
        write!(
            f,
            [token("assert"), space(), flat_test, token(","), space()]
        )?;
        parenthesized("(", &message.format().with_options(Parentheses::Never), ")")
            .with_force_expansion(true)
            .fmt(f)
    })
    .memoized();
    let test_split = format_with(|f| {
        write!(
            f,
            [
                token("assert"),
                space(),
                parenthesized(
                    "(",
                    &item.test.format().with_options(Parentheses::Never),
                    ")"
                )
                .with_force_expansion(true),
                token(","),
                space()
            ]
        )?;
        if multiline {
            parenthesized("(", &message.format().with_options(Parentheses::Never), ")").fmt(f)
        } else {
            maybe_parenthesize_expression(message, item, Parenthesize::IfRequired).fmt(f)
        }
    });
    if multiline && measurement::first_line_fits(message_split.inspect(f)?, f.context())? {
        message_split.fmt(f)?;
    } else {
        test_split.fmt(f)?;
    }
    Ok(true)
}
