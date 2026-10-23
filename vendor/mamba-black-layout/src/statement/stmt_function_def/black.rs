//! Black's choice of the first split point in a function header.
use ruff_formatter::{FormatContext, FormatOptions};
use ruff_python_ast::{
    Expr, StmtFunctionDef,
    token::TokenKind,
    visitor::{self, Visitor},
};
use ruff_text_size::{Ranged, TextRange};

use crate::expression::parentheses::{Parentheses, parenthesized};
use crate::prelude::*;
use crate::{PyFormatContext, other::commas::has_magic_trailing_comma};
use ruff_formatter::{format_args, write};
mod measurement;

struct Magic<'a, 's> {
    context: &'a PyFormatContext<'s>,
    found: bool,
}

impl<'s> Visitor<'s> for Magic<'_, 's> {
    fn visit_expr(&mut self, expression: &'s Expr) {
        let last = match expression {
            Expr::Subscript(value) => value
                .slice
                .as_tuple_expr()
                .filter(|tuple| tuple.len() > 1)
                .and_then(|tuple| tuple.elts.last())
                .map(Ranged::end),
            Expr::Tuple(value) if value.len() > 1 => value.elts.last().map(Ranged::end),
            Expr::List(value) => value.elts.last().map(Ranged::end),
            Expr::Set(value) => value.elts.last().map(Ranged::end),
            Expr::Dict(value) => value.items.last().map(|item| item.value.end()),
            _ => None,
        };
        if last.is_some_and(|end| {
            has_magic_trailing_comma(TextRange::new(end, expression.end()), self.context)
        }) {
            self.found = true;
        } else if !self.found {
            visitor::walk_expr(self, expression);
        }
    }
}

pub(super) fn split_return_first<'s>(expression: &'s Expr, context: &PyFormatContext<'s>) -> bool {
    if context.options().style_policy().is_none() {
        return false;
    }
    let mut magic = Magic {
        context,
        found: false,
    };
    magic.visit_expr(expression);
    magic.found
        || expression.as_string_literal_expr().is_some_and(|literal| {
            !literal.value.is_implicit_concatenated()
                && context
                    .options()
                    .style_policy()
                    .unwrap()
                    .literal(&context.source()[expression.range()])
                    .chars()
                    .count()
                    > usize::from(context.options().line_width().value()).saturating_sub(
                        context
                            .indent_level()
                            .to_ascii_spaces(context.options().indent_width())
                            as usize,
                    )
        })
}

// Measure formatted tokens. Source spaces and literal spelling cannot set the split.
pub(super) fn format_return_subscript(expression: &Expr, f: &mut PyFormatter) -> FormatResult<()> {
    let subscript = expression.as_subscript_expr().unwrap();
    let Some(tuple) = subscript.slice.as_tuple_expr().filter(|tuple| {
        tuple.len() > 1 && !tuple.parenthesized && !f.context().comments().has(*tuple)
    }) else {
        return crate::builders::parenthesize_if_expands(
            &expression.format().with_options(Parentheses::Never),
        )
        .fmt(f);
    };
    let items = format_with(|f: &mut PyFormatter| {
        let mut f = crate::context::WithNodeLevel::new(
            crate::context::NodeLevel::ParenthesizedExpression,
            f,
        );
        f.join_comma_separated(tuple.end())
            .nodes(&tuple.elts)
            .finish()
    })
    .memoized();
    let measured = group(&items).memoized();
    let width = measurement::flat_width(measured.inspect(f)?, f.context())?;
    let available = usize::from(f.options().line_width().value()).saturating_sub(
        f.context()
            .indent_level()
            .to_ascii_spaces(f.options().indent_width()) as usize
            + 8,
    );
    let wrap = width.is_some_and(|width| width <= available);
    let body = format_with(|f| {
        if wrap {
            group(&items).fmt(f)
        } else {
            items.fmt(f)
        }
    });
    let value = format_with(|f| {
        write!(
            f,
            [
                subscript.value.format().with_options(Parentheses::Never),
                parenthesized(
                    "[",
                    &format_args![(!wrap).then_some(expand_parent()), body],
                    "]"
                )
            ]
        )
    });
    if wrap {
        crate::builders::parenthesize_if_expands(&value).fmt(f)
    } else {
        value.fmt(f)
    }
}

pub(super) fn parameter_magic(item: &StmtFunctionDef, context: &PyFormatContext<'_>) -> bool {
    let mut tokens = context
        .tokens()
        .in_range(item.parameters.range())
        .iter()
        .rev()
        .filter(|token| !token.kind().is_trivia() && !token.kind().is_any_newline());
    tokens
        .next()
        .is_some_and(|token| token.kind() == TokenKind::Rpar)
        && tokens
            .next()
            .is_some_and(|token| token.kind() == TokenKind::Comma)
}

pub(super) fn type_parameter_magic(item: &StmtFunctionDef, context: &PyFormatContext<'_>) -> bool {
    if context.options().style_policy().is_none() {
        return false;
    }
    let Some(parameters) = item.type_params.as_deref() else {
        return false;
    };
    let mut tokens = context
        .tokens()
        .in_range(parameters.range())
        .iter()
        .rev()
        .filter(|token| !token.kind().is_trivia() && !token.kind().is_any_newline());
    tokens
        .next()
        .is_some_and(|token| token.kind() == TokenKind::Rsqb)
        && tokens
            .next()
            .is_some_and(|token| token.kind() == TokenKind::Comma)
}

pub(super) fn unsplittable(item: &StmtFunctionDef, context: &PyFormatContext<'_>) -> bool {
    if context.options().style_policy().is_none()
        || parameter_magic(item, context)
        || type_parameter_magic(item, context)
        || item.returns.as_deref().is_some_and(|returns| {
            let mut magic = Magic {
                context,
                found: false,
            };
            magic.visit_expr(returns);
            magic.found
        })
    {
        return false;
    }
    let Ok(range) = crate::statement::clause::ClauseHeader::Function(item).range(context.source())
    else {
        return false;
    };
    if context.source()[range].contains(['\r', '\n']) {
        return false;
    }
    context
        .tokens()
        .in_range(TextRange::new(range.end(), item.end()))
        .iter()
        .take_while(|token| token.kind() != TokenKind::Newline)
        .any(|token| {
            token.kind() == TokenKind::Comment
                && crate::black_comments::is_ignore(&context.source()[token.range()])
        })
        || context
            .comments()
            .dangling(item)
            .iter()
            .chain(context.comments().trailing(item))
            .filter(|comment| comment.start() >= range.end())
            .any(|comment| {
                crate::black_comments::is_ignore(&context.source()[comment.range()])
                    && !context.source()[TextRange::new(range.start(), comment.start())]
                        .contains(['\r', '\n'])
            })
}
