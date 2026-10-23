//! Preserve physical type-ignore lines when a complete assignment cannot fit.
use crate::comments::{Comments, SourceComment, trailing_comments};
use crate::context::with_forced_bracket;
use crate::expression::parentheses::Parentheses;
use crate::prelude::*;
use ruff_formatter::{RemoveSoftLinesBuffer, write};
use ruff_python_ast::{
    AnyNodeRef, Expr, StmtAssign,
    token::{TokenKind, Tokens},
    visitor::source_order::{SourceOrderVisitor, TraversalSignal},
};
use ruff_text_size::{Ranged, TextRange};
mod chain;

struct Snapshot<'a, 'ast> {
    comments: &'a Comments<'ast>,
    source: &'ast str,
    tokens: &'ast Tokens,
    value_range: TextRange,
    states: Vec<(&'a SourceComment, bool)>,
    opening: Option<TextRange>,
    ignores: bool,
    other_comments: bool,
}

impl Snapshot<'_, '_> {
    fn restore(&self) {
        for (comment, formatted) in &self.states {
            if *formatted {
                comment.mark_formatted();
            } else {
                comment.mark_unformatted();
            }
        }
    }
}

impl<'ast> SourceOrderVisitor<'ast> for Snapshot<'_, 'ast> {
    fn enter_node(&mut self, node: AnyNodeRef<'ast>) -> TraversalSignal {
        for comment in self.comments.leading_dangling_trailing(node) {
            self.states.push((comment, comment.is_formatted()));
            if comment.is_formatted() {
                continue;
            }
            self.ignores |= comment.line_position().is_end_of_line()
                && crate::black_comments::is_ignore(&self.source[comment.range()]);
            self.other_comments |= !crate::black_comments::is_ignore(&self.source[comment.range()]);
        }
        let bracket = match node {
            AnyNodeRef::Arguments(arguments) => Some(arguments.range()),
            AnyNodeRef::ExprSubscript(index) => self
                .tokens
                .in_range(TextRange::new(index.value.end(), index.end()))
                .iter()
                .find(|token| token.kind() == TokenKind::Lsqb)
                .map(|open| TextRange::new(open.start(), index.end())),
            _ => None,
        };
        if self.opening.is_none()
            && bracket.is_some_and(|range| self.value_range.contains_range(range))
            && self.comments.dangling(node).iter().any(|comment| {
                comment.line_position().is_end_of_line()
                    && crate::black_comments::is_ignore(&self.source[comment.range()])
            })
        {
            self.opening = bracket;
        }
        TraversalSignal::Traverse
    }
}

pub(super) fn format(item: &StmtAssign, f: &mut PyFormatter) -> FormatResult<bool> {
    if f.options().style_policy().is_none() || f.context().forced_bracket().is_some() {
        return Ok(false);
    }
    let lambda_target = if item.value.is_lambda_expr()
        && !f.context().source()[item.value.range()].contains(['\r', '\n'])
        && !f
            .context()
            .is_expression_parenthesized(item.value.as_ref().into())
        && let [Expr::Subscript(target)] = item.targets.as_slice()
        && f.context().source()[target.range()].contains(['\r', '\n'])
    {
        f.context()
            .tokens()
            .in_range(TextRange::new(target.value.end(), target.end()))
            .iter()
            .find(|token| token.kind() == TokenKind::Lsqb)
            .map(|open| TextRange::new(open.start(), target.end()))
    } else {
        None
    };
    let ranges = crate::expression::black_trailers::chain_brackets(&item.value, f.context());
    let call_chain = ranges.len() > 1;
    if lambda_target.is_none() && !call_chain {
        return Ok(false);
    }
    let comments = f.context().comments().clone();
    let mut snapshot = Snapshot {
        comments: &comments,
        source: f.context().source(),
        tokens: f.context().tokens(),
        value_range: item.value.range(),
        states: Vec::new(),
        opening: None,
        ignores: false,
        other_comments: false,
    };
    let node = AnyNodeRef::from(item);
    snapshot.enter_node(node);
    node.visit_source_order(&mut snapshot);
    if !snapshot.ignores || snapshot.other_comments {
        return Ok(false);
    }
    let opening = call_chain
        .then_some(snapshot.opening)
        .flatten()
        .filter(|range| ranges.contains(range));
    let Some(selected) = lambda_target.or(opening) else {
        return Ok(false);
    };
    if let Some(opening) = opening {
        return chain::format(item, opening, &ranges, &snapshot, f);
    }
    let flat = format_with(|f| {
        with_forced_bracket(TextRange::empty(0.into()), f, |f| {
            super::FormatStmtAssign.fmt_fields(item, f)
        })?;
        trailing_comments(comments.trailing(item)).fmt(f)
    })
    .memoized();
    if crate::black_measurement::flat_line_fits(flat.inspect(f)?, f.context())? {
        flat.fmt(f)?;
    } else {
        snapshot.restore();
        with_forced_bracket(selected, f, |f| {
            if lambda_target.is_some()
                && comments.trailing(item).iter().any(|comment| {
                    comment.line_position().is_end_of_line()
                        && crate::black_comments::is_ignore(&f.context().source()[comment.range()])
                })
            {
                super::FormatTargetWithEqualOperator {
                    target: &item.targets[0],
                    preserve_parentheses: true,
                }
                .fmt(f)?;
                let mut buffer = RemoveSoftLinesBuffer::new(f);
                write!(
                    buffer,
                    [item.value.format().with_options(Parentheses::Never)]
                )
            } else {
                super::FormatStmtAssign.fmt_fields(item, f)
            }
        })?;
    }
    Ok(true)
}
