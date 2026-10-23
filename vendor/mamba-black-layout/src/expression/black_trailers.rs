//! Stable Black's bracket omission in chains after a magic comma.
use crate::prelude::*;
use ruff_formatter::{FormatContext, FormatOptions};
use ruff_python_ast::{
    AnyNodeRef, Expr, ExprTuple,
    token::{TokenKind, parentheses_iterator},
    visitor::{self, Visitor},
};
use ruff_text_size::{Ranged, TextLen, TextRange};

struct Bracket {
    range: TextRange,
    magic: bool,
}

pub(crate) fn is_fluent(expr: &Expr, context: &PyFormatContext) -> bool {
    super::CallChainLayout::from_expression(expr.into(), context).is_fluent()
}

pub(crate) fn chain_brackets(expr: &Expr, context: &PyFormatContext) -> Vec<TextRange> {
    if !matches!(
        expr,
        Expr::Call(_) | Expr::Subscript(_) | Expr::Attribute(_)
    ) {
        return Vec::new();
    }
    let mut ranges = Vec::new();
    chain(expr, context, &mut ranges, true);
    ranges.into_iter().map(|bracket| bracket.range).collect()
}

fn chain(expr: &Expr, context: &PyFormatContext, ranges: &mut Vec<Bracket>, root: bool) {
    if !root && context.is_expression_parenthesized(expr.into()) {
        if let Some(range) = parentheses_iterator(expr.into(), None, context.tokens()).last() {
            ranges.push(Bracket {
                range,
                magic: has_magic(expr, context),
            });
            return;
        }
    }
    let range = match expr {
        Expr::Call(call) => {
            chain(&call.func, context, ranges, false);
            (!call.arguments.is_empty()).then_some(call.arguments.range())
        }
        Expr::Subscript(subscript) => {
            chain(&subscript.value, context, ranges, false);
            context
                .tokens()
                .in_range(TextRange::new(subscript.value.end(), subscript.end()))
                .iter()
                .find(|token| token.kind() == TokenKind::Lsqb)
                .map(|open| TextRange::new(open.start(), subscript.end()))
        }
        Expr::Attribute(attribute) => {
            chain(&attribute.value, context, ranges, false);
            None
        }
        Expr::List(list) if !list.elts.is_empty() => Some(expr.range()),
        Expr::Tuple(tuple) if !tuple.elts.is_empty() => Some(expr.range()),
        Expr::Set(set) if !set.elts.is_empty() => Some(expr.range()),
        Expr::Dict(dict) if !dict.items.is_empty() => Some(expr.range()),
        _ => None,
    };
    if let Some(range) = range {
        let magic = match expr {
            Expr::Call(call) => {
                magic(range, context)
                    || call
                        .arguments
                        .args
                        .iter()
                        .any(|arg| has_magic(arg, context))
                    || call
                        .arguments
                        .keywords
                        .iter()
                        .any(|keyword| has_magic(&keyword.value, context))
            }
            Expr::Subscript(subscript) => {
                (subscript
                    .slice
                    .as_tuple_expr()
                    .is_none_or(|tuple| tuple.elts.len() > 1)
                    && magic(range, context))
                    || has_magic(&subscript.slice, context)
            }
            _ => has_magic(expr, context),
        };
        ranges.push(Bracket { range, magic });
    }
}

struct Magic<'a, 'ast> {
    context: &'a PyFormatContext<'ast>,
    found: bool,
}

fn tuple_magic(tuple: &ExprTuple, context: &PyFormatContext) -> bool {
    tuple.elts.len() > 1
        && tuple.elts.last().is_some_and(|last| {
            crate::other::commas::has_magic_trailing_comma(
                TextRange::new(last.end(), tuple.end()),
                context,
            )
        })
}

impl<'expr> Visitor<'expr> for Magic<'_, '_> {
    fn visit_expr(&mut self, expr: &'expr Expr) {
        self.found |= match expr {
            Expr::Call(call) => magic(call.arguments.range(), self.context),
            Expr::Subscript(subscript) => {
                subscript
                    .slice
                    .as_tuple_expr()
                    .is_none_or(|tuple| tuple.elts.len() > 1)
                    && magic(subscript.range(), self.context)
            }
            Expr::List(_) | Expr::Set(_) | Expr::Dict(_) => magic(expr.range(), self.context),
            Expr::Tuple(tuple) => tuple_magic(tuple, self.context),
            _ => false,
        };
        if !self.found {
            visitor::walk_expr(self, expr);
        }
    }
}

pub(crate) fn has_magic(expr: &Expr, context: &PyFormatContext) -> bool {
    let mut visitor = Magic {
        context,
        found: false,
    };
    visitor.visit_expr(expr);
    visitor.found
}

pub(crate) fn has_breaking_magic(node: AnyNodeRef, context: &PyFormatContext) -> bool {
    match node {
        AnyNodeRef::ExprCall(call) => {
            magic(call.arguments.range(), context)
                || has_magic(&call.func, context)
                || call
                    .arguments
                    .args
                    .iter()
                    .any(|arg| has_magic(arg, context))
                || call
                    .arguments
                    .keywords
                    .iter()
                    .any(|keyword| has_magic(&keyword.value, context))
        }
        AnyNodeRef::ExprSubscript(subscript) => {
            has_magic(&subscript.value, context)
                || has_magic(&subscript.slice, context)
                || subscript
                    .slice
                    .as_tuple_expr()
                    .is_none_or(|tuple| tuple.elts.len() > 1)
                    && magic(subscript.range(), context)
        }
        AnyNodeRef::ExprAttribute(attribute) => has_magic(&attribute.value, context),
        AnyNodeRef::ExprList(list) => {
            magic(list.range(), context) || list.elts.iter().any(|value| has_magic(value, context))
        }
        AnyNodeRef::ExprSet(set) => {
            magic(set.range(), context) || set.elts.iter().any(|value| has_magic(value, context))
        }
        AnyNodeRef::ExprDict(dict) => {
            magic(dict.range(), context)
                || dict.items.iter().any(|item| {
                    item.key.as_ref().is_some_and(|key| has_magic(key, context))
                        || has_magic(&item.value, context)
                })
        }
        AnyNodeRef::ExprTuple(tuple) => {
            tuple_magic(tuple, context) || tuple.elts.iter().any(|value| has_magic(value, context))
        }
        AnyNodeRef::StmtAssign(assign) => {
            assign
                .targets
                .iter()
                .any(|target| has_magic(target, context))
                || has_magic(&assign.value, context)
        }
        AnyNodeRef::StmtReturn(ret) => ret
            .value
            .as_deref()
            .is_some_and(|value| has_magic(value, context)),
        AnyNodeRef::StmtAnnAssign(assign) => {
            has_magic(&assign.target, context)
                || assign
                    .value
                    .as_deref()
                    .is_some_and(|value| has_magic(value, context))
        }
        AnyNodeRef::StmtAugAssign(assign) => {
            has_magic(&assign.target, context) || has_magic(&assign.value, context)
        }
        AnyNodeRef::StmtExpr(expr) => has_magic(&expr.value, context),
        _ => false,
    }
}

pub(crate) fn last_chain_bracket(expr: &Expr, context: &PyFormatContext) -> Option<TextRange> {
    if !matches!(
        expr,
        Expr::Call(_) | Expr::Subscript(_) | Expr::Attribute(_)
    ) {
        return None;
    }
    let mut ranges = Vec::new();
    chain(expr, context, &mut ranges, true);
    ranges.last().map(|bracket| bracket.range)
}

fn suffix_width(range: TextRange, context: &PyFormatContext) -> usize {
    let mut width = usize::from(
        context
            .indent_level()
            .to_ascii_spaces(context.options().indent_width()),
    );
    let mut previous = None;
    for token in context
        .tokens()
        .in_range(range)
        .iter()
        .filter(|token| !token.kind().is_trivia())
    {
        if let Some(previous_token) = previous {
            let previous_token: &ruff_python_ast::token::Token = previous_token;
            let closing = matches!(
                token.kind(),
                TokenKind::Rpar
                    | TokenKind::Rsqb
                    | TokenKind::Rbrace
                    | TokenKind::Comma
                    | TokenKind::Dot
                    | TokenKind::Colon
            );
            let opening = matches!(
                previous_token.kind(),
                TokenKind::Lpar | TokenKind::Lsqb | TokenKind::Lbrace | TokenKind::Dot
            );
            let gap = &context.source()[TextRange::new(previous_token.end(), token.start())];
            if !closing
                && !opening
                && (previous_token.kind() == TokenKind::Comma || !gap.is_empty())
            {
                width += 1;
            }
        }
        width += ruff_formatter::black_width::measure(&context.source()[token.range()]);
        previous = Some(token);
    }
    width
}

fn magic(range: TextRange, context: &PyFormatContext) -> bool {
    let mut tokens = context
        .tokens()
        .in_range(range)
        .iter()
        .rev()
        .filter(|token| !token.kind().is_trivia());
    tokens.next().is_some_and(|token| {
        matches!(
            token.kind(),
            TokenKind::Rpar | TokenKind::Rsqb | TokenKind::Rbrace
        )
    }) && tokens
        .next()
        .is_some_and(|token| token.kind() == TokenKind::Comma)
}

pub(super) fn selected(expr: &Expr, context: &PyFormatContext) -> Option<TextRange> {
    if context.options().style_policy().is_none()
        || context.forced_bracket().is_some()
        || !matches!(
            expr,
            Expr::Call(_) | Expr::Subscript(_) | Expr::Attribute(_)
        )
        || super::CallChainLayout::from_expression(expr.into(), context).is_fluent()
    {
        return None;
    }
    let mut ranges = Vec::new();
    chain(expr, context, &mut ranges, true);
    let last_magic = ranges.iter().rposition(|bracket| bracket.magic)?;
    if ranges.len() - last_magic < 3 {
        return None;
    }
    let tail = &context.source()[TextRange::new(expr.end(), context.source().text_len())];
    let has_comment = tail
        .split(['\r', '\n'])
        .next()
        .unwrap_or("")
        .trim_start()
        .starts_with('#');
    let selected = if has_comment {
        ranges.last()?.range
    } else {
        ranges[ranges.len() - 2].range
    };
    if suffix_width(TextRange::new(expr.start(), selected.start()), context) + 1
        > usize::from(context.options().line_width().value())
    {
        return None;
    }
    (suffix_width(TextRange::new(selected.end(), expr.end()), context)
        <= usize::from(context.options().line_width().value()))
    .then_some(selected)
}
