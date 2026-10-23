//! Reproduce stable Black's tuple normalization before the caller's AST check.
use crate::prelude::*;
use ruff_formatter::FormatContext;
use ruff_python_ast::{Expr, StmtWith, StringLike, token::parentheses_iterator};
use ruff_python_trivia::{SimpleTokenKind, SimpleTokenizer};
use ruff_text_size::{Ranged, TextRange};

pub(super) fn flattened(with: &StmtWith, context: &PyFormatContext) -> Vec<TextRange> {
    if context.options().style_policy().is_none() {
        return Vec::new();
    }
    let (first_atom, wrapped) = prefix(with, context);
    with.items
        .iter()
        .filter_map(|item| {
            let tuple = item.context_expr.as_tuple_expr()?;
            if tuple.elts.len() < 2
                || item.optional_vars.is_some()
                || tuple
                    .elts
                    .iter()
                    .any(|expr| matches!(expr, Expr::Named(_) | Expr::Starred(_)))
                || context.comments().contains_comments(tuple.into())
            {
                return None;
            }
            let nested = parentheses_iterator(tuple.into(), None, context.tokens())
                .next()
                .is_some();
            (wrapped || !first_atom || nested).then_some(item.range())
        })
        .collect()
}

pub(super) fn atom_prefix(with: &StmtWith, context: &PyFormatContext) -> bool {
    let (atom, wrapped) = prefix(with, context);
    context.options().style_policy().is_some() && atom && !wrapped
}

fn prefix(with: &StmtWith, context: &PyFormatContext) -> (bool, bool) {
    let Some(first) = with.items.first() else {
        return (false, false);
    };
    let first_expr = &first.context_expr;
    let first_atom = first.optional_vars.is_none()
        && (matches!(
            first_expr,
            Expr::Tuple(_) | Expr::List(_) | Expr::Set(_) | Expr::Dict(_) | Expr::Generator(_)
        ) || context.is_expression_parenthesized(first_expr.into())
            || StringLike::try_from(first_expr)
                .is_ok_and(|string| string.is_implicit_concatenated()));
    let mut tokens = SimpleTokenizer::new(
        context.source(),
        TextRange::new(with.start(), first.start()),
    )
    .skip_trivia()
    .skip_while(|token| token.kind() == SimpleTokenKind::Async);
    let _with_keyword = tokens.next();
    let wrapped = tokens
        .next()
        .is_some_and(|token| token.kind() == SimpleTokenKind::LParen);
    (first_atom, wrapped)
}
