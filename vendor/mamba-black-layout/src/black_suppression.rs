//! Black's `fmt: skip` on an atom's opening bracket freezes the whole atom.
use crate::comments::{Comments, leading_comments, trailing_comments};
use crate::prelude::*;
use ruff_formatter::format_args;
use ruff_python_ast::visitor::source_order::{SourceOrderVisitor, TraversalSignal};
use ruff_python_ast::{
    AnyNodeRef, Expr, Stmt,
    token::{TokenKind, parentheses_iterator},
};
use ruff_python_trivia::SuppressionKind;
use ruff_text_size::{Ranged, TextRange};
pub(crate) mod line;
pub(crate) mod off;
pub(crate) mod opening;

pub(crate) fn format_skipped_atom(
    expression: &Expr,
    allow_parens: bool,
    f: &mut PyFormatter,
) -> FormatResult<bool> {
    format_skipped_node(expression.into(), expression, allow_parens, f)
}

pub(crate) fn format_skipped_docstring(
    statement: &Stmt,
    f: &mut PyFormatter,
) -> FormatResult<bool> {
    let Some(statement_expression) = statement.as_expr_stmt() else {
        return Ok(false);
    };
    format_skipped_node(statement.into(), &statement_expression.value, true, f)
}

fn format_skipped_node(
    node: AnyNodeRef,
    expression: &Expr,
    allow_parens: bool,
    f: &mut PyFormatter,
) -> FormatResult<bool> {
    if f.options().style_policy().is_none() {
        return Ok(false);
    }
    let range = if allow_parens && f.context().is_expression_parenthesized(expression.into()) {
        parentheses_iterator(expression.into(), None, f.context().tokens())
            .last()
            .unwrap_or_else(|| expression.range())
    } else {
        expression.range()
    };
    let mut tokens = f.context().tokens().in_range(range).iter();
    if !tokens.next().is_some_and(|token| {
        matches!(
            token.kind(),
            TokenKind::Lpar | TokenKind::Lsqb | TokenKind::Lbrace
        )
    }) {
        return Ok(false);
    }
    let mut skipped = false;
    for token in tokens {
        match token.kind() {
            TokenKind::Comment => {
                let text = &f.context().source()[token.range()];
                skipped |=
                    crate::comments::suppression::controls(text).contains(&SuppressionKind::Skip);
            }
            TokenKind::Newline
            | TokenKind::NonLogicalNewline
            | TokenKind::Indent
            | TokenKind::Dedent => {}
            _ => break,
        }
    }
    let tuple_items = if skipped {
        None
    } else {
        line::tail_tuple(expression, range, f.context())
    };
    if !skipped && tuple_items.is_none() {
        return Ok(false);
    }
    let comments = f.context().comments().clone();
    for comment in comments.leading(node) {
        if comment.end() <= range.start() {
            leading_comments(std::slice::from_ref(comment)).fmt(f)?;
        }
    }
    struct Mark<'a> {
        comments: &'a Comments<'a>,
        range: TextRange,
    }
    impl<'a> SourceOrderVisitor<'a> for Mark<'a> {
        fn enter_node(&mut self, node: AnyNodeRef<'a>) -> TraversalSignal {
            for comment in self.comments.leading_dangling_trailing(node) {
                if self.range.contains_range(comment.range()) {
                    comment.mark_formatted();
                }
            }
            TraversalSignal::Traverse
        }
    }
    let mut marker = Mark {
        comments: &comments,
        range,
    };
    for comment in comments.leading_dangling_trailing(node) {
        if range.contains_range(comment.range()) {
            comment.mark_formatted();
        }
    }
    node.visit_source_order(&mut marker);
    if let Some(tuple_items) = tuple_items {
        let (left, right) = match expression {
            Expr::List(_) => ("[", "]"),
            Expr::Set(_) | Expr::Dict(_) => ("{", "}"),
            _ => ("(", ")"),
        };
        crate::expression::parentheses::parenthesized(
            left,
            &format_args![expand_parent(), crate::verbatim::verbatim_text(tuple_items)],
            right,
        )
        .fmt(f)?;
    } else {
        crate::verbatim::verbatim_text(range).fmt(f)?;
    }
    for comment in comments.trailing(node) {
        if comment.start() >= range.end() {
            trailing_comments(std::slice::from_ref(comment)).fmt(f)?;
        }
    }
    Ok(true)
}

pub(crate) fn header_skip_bracket(range: TextRange, context: &PyFormatContext) -> bool {
    let mut opened = false;
    for token in context.tokens().in_range(range).iter() {
        if opened {
            if token.kind() == TokenKind::Comment {
                return crate::comments::suppression::controls(&context.source()[token.range()])
                    .contains(&SuppressionKind::Skip);
            }
            if token.kind().is_trivia() {
                continue;
            }
            return false;
        }
        match token.kind() {
            TokenKind::Lpar => opened = true,
            TokenKind::Async | TokenKind::With | TokenKind::Except | TokenKind::Star => {}
            _ if &context.source()[token.range()] == "case" => {}
            _ if token.kind().is_trivia() => {}
            _ => return false,
        }
    }
    false
}
