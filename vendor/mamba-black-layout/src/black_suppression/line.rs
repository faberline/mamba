//! A skipped physical line retains its code and comment spacing.
use crate::comments::Comments;
use crate::prelude::*;
use ruff_formatter::FormatContext;
use ruff_python_ast::visitor::source_order::{SourceOrderVisitor, TraversalSignal};
use ruff_python_ast::{AnyNodeRef, token::TokenKind};
use ruff_python_trivia::SuppressionKind;
use ruff_text_size::{Ranged, TextLen, TextRange};

pub(crate) fn skip_after(range: TextRange, context: &PyFormatContext) -> bool {
    if context.options().style_policy().is_none() || context.source()[range].contains(['\r', '\n'])
    {
        return false;
    }
    for token in context
        .tokens()
        .in_range(TextRange::new(range.end(), context.source().text_len()))
        .iter()
    {
        match token.kind() {
            TokenKind::Comment => {
                return crate::comments::suppression::controls(&context.source()[token.range()])
                    .contains(&SuppressionKind::Skip);
            }
            TokenKind::Comma | TokenKind::Rpar | TokenKind::Rsqb | TokenKind::Rbrace => {}
            _ => return false,
        }
    }
    false
}

pub(crate) fn format_fields(node: AnyNodeRef, f: &mut PyFormatter) -> FormatResult<bool> {
    if node
        .as_expr_compare()
        .is_some_and(|compare| skipped_comparison(compare, f.context()))
    {
        mark(node, node.range(), f.context().comments());
        crate::verbatim::verbatim_text(node.range()).fmt(f)?;
        return Ok(true);
    }
    if !(node.is_expression()
        || node.is_keyword()
        || matches!(
            node,
            AnyNodeRef::StringLiteral(_)
                | AnyNodeRef::BytesLiteral(_)
                | AnyNodeRef::FString(_)
                | AnyNodeRef::TString(_)
        ))
        || !skip_after(node.range(), f.context())
    {
        return Ok(false);
    }
    mark(node, node.range(), f.context().comments());
    crate::verbatim::verbatim_text(node.range()).fmt(f)?;
    Ok(true)
}

/// A skipped bracket atom can cover its comparison prefix on the same line.
pub(crate) fn skipped_comparison(
    compare: &ruff_python_ast::ExprCompare,
    context: &PyFormatContext,
) -> bool {
    if context.options().style_policy().is_none() || compare.ops.len() != 1 {
        return false;
    }
    let right = compare.second_operand();
    if !matches!(
        right,
        ruff_python_ast::Expr::Tuple(_)
            | ruff_python_ast::Expr::List(_)
            | ruff_python_ast::Expr::Set(_)
            | ruff_python_ast::Expr::Dict(_)
    ) {
        return false;
    }
    let range = ruff_python_ast::token::parentheses_iterator(right.into(), None, context.tokens())
        .last()
        .unwrap_or_else(|| right.range());
    if range.start() < compare.first_operand().end()
        || context.source()[TextRange::new(compare.first_operand().end(), range.start())]
            .contains(['\r', '\n'])
    {
        return false;
    }
    let closing_skip = context
        .tokens()
        .in_range(TextRange::new(range.end(), context.source().text_len()))
        .iter()
        .take_while(|token| {
            matches!(
                token.kind(),
                TokenKind::Comment | TokenKind::Rpar | TokenKind::Rsqb | TokenKind::Rbrace
            )
        })
        .find(|token| token.kind() == TokenKind::Comment)
        .is_some_and(|token| {
            crate::comments::suppression::controls(&context.source()[token.range()])
                .contains(&SuppressionKind::Skip)
        });
    super::opening::first_comment_skips(compare.range(), context) || closing_skip
}

pub(crate) fn skipped_collection_tail(
    expression: &ruff_python_ast::Expr,
    context: &PyFormatContext,
) -> bool {
    if context.options().style_policy().is_none() {
        return false;
    }
    let range =
        ruff_python_ast::token::parentheses_iterator(expression.into(), None, context.tokens())
            .last()
            .unwrap_or_else(|| expression.range());
    tail_tuple(expression, range, context).is_some()
}

pub(crate) fn skipped_definition(node: AnyNodeRef, context: &PyFormatContext) -> bool {
    if context.options().style_policy().is_none() {
        return false;
    }
    let body = match node {
        AnyNodeRef::StmtFunctionDef(function) => &function.body,
        AnyNodeRef::StmtClassDef(class) => &class.body,
        _ => return false,
    };
    let Some(last) = body.last() else {
        return false;
    };
    !context.source()[TextRange::new(node.start(), last.end())].contains(['\r', '\n'])
        && crate::comments::has_skip_comment(context.comments().trailing(last), context.source())
}

/// A final skip before a collection's closing bracket covers its complete item list.
pub(crate) fn tail_tuple(
    expression: &ruff_python_ast::Expr,
    range: TextRange,
    context: &PyFormatContext,
) -> Option<TextRange> {
    let (first, last) = match expression {
        ruff_python_ast::Expr::Tuple(value) => {
            (value.elts.first()?.range(), value.elts.last()?.range())
        }
        ruff_python_ast::Expr::List(value) => {
            (value.elts.first()?.range(), value.elts.last()?.range())
        }
        ruff_python_ast::Expr::Set(value) => {
            (value.elts.first()?.range(), value.elts.last()?.range())
        }
        ruff_python_ast::Expr::Dict(value) => {
            let first = value.items.first()?;
            let first_range = if first.key.is_none() {
                let operator = context
                    .tokens()
                    .in_range(TextRange::new(range.start(), first.start()))
                    .iter()
                    .find(|token| token.kind() == TokenKind::DoubleStar)?;
                TextRange::new(operator.start(), first.end())
            } else {
                first.range()
            };
            (first_range, value.items.last()?.range())
        }
        _ => return None,
    };
    if !context.source()[TextRange::new(range.start(), first.start())].contains('\n')
        || context
            .tokens()
            .in_range(TextRange::new(range.start(), first.start()))
            .iter()
            .any(|token| token.kind() == TokenKind::Comment)
        || !skip_after(last, context)
    {
        return None;
    }
    let comment = context
        .tokens()
        .in_range(TextRange::new(last.end(), range.end()))
        .iter()
        .find(|token| token.kind() == TokenKind::Comment)?;
    if context
        .tokens()
        .in_range(TextRange::new(comment.end(), range.end()))
        .iter()
        .any(|token| {
            !matches!(
                token.kind(),
                TokenKind::NonLogicalNewline
                    | TokenKind::Rpar
                    | TokenKind::Rsqb
                    | TokenKind::Rbrace
            )
        })
    {
        return None;
    }
    Some(TextRange::new(first.start(), comment.end()))
}

pub(crate) fn mark<'a>(node: AnyNodeRef<'a>, range: TextRange, comments: &Comments<'a>) {
    struct Marker<'a, 'b> {
        range: TextRange,
        comments: &'b Comments<'a>,
    }
    impl<'a> SourceOrderVisitor<'a> for Marker<'a, '_> {
        fn enter_node(&mut self, node: AnyNodeRef<'a>) -> TraversalSignal {
            for comment in self.comments.leading_dangling_trailing(node) {
                if self.range.contains_range(comment.range()) {
                    comment.mark_formatted();
                }
            }
            TraversalSignal::Traverse
        }
    }
    let mut marker = Marker { range, comments };
    marker.enter_node(node);
    node.visit_source_order(&mut marker);
}
