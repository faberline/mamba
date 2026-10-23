use super::ast::*;
use super::Parser;
use crate::source::span::{Span, Spanned};
use mamba_shared_kernel::token::TokenKind;

#[cfg(test)]
mod tests;

mod at_comprehension_clause_start;
mod collect_walrus_targets;
mod collect_walrus_targets_in_fstring_part;
mod consume_balanced_target_trailer;
mod expr_contains_walrus;
mod parse_await_expr;
mod parse_brace_expr;
mod parse_comprehension_clauses;
mod parse_comprehension_target_atom;
mod parse_comprehension_target_list_until;
mod parse_comprehension_targets;
mod parse_lambda;
mod parse_list_or_comp;
mod parse_paren_expr;
mod parse_ternary;
mod parse_yield_expr;
mod validate_comprehension_assignment_exprs;
