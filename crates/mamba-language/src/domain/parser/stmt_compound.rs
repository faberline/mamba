use super::ast::*;
use super::Parser;
use crate::error::MambaError;
use crate::source::span::{Span, Spanned};
use mamba_shared_kernel::token::TokenKind;

#[cfg(test)]
mod tests;

// ── match-pattern static validation (CPython compile-time SyntaxErrors) ──

/// Literal mapping-pattern key, normalized so CPython's value-equality
/// duplicate rule holds: 0 == False == 0.0 == -0.
enum MatchKey {
    Num(f64),
    Str(String),
    NoneKey,
}

mod is_parenthesized_with;
mod match_key_of;
mod match_keys_equal;
mod parse_assert;
mod parse_async_fn_def;
mod parse_async_stmt;
mod parse_class_def;
mod parse_decorated;
mod parse_del;
mod parse_enum_def;
mod parse_fn_def;
mod parse_for;
mod parse_global;
mod parse_if;
mod parse_match;
mod parse_nonlocal;
mod parse_raise;
mod parse_try;
mod parse_type_alias;
mod parse_while;
mod parse_with;
mod pattern_is_irrefutable;
mod validate_match_arms;
mod validate_pattern;

use match_key_of::match_key_of;
use match_keys_equal::match_keys_equal;
use pattern_is_irrefutable::pattern_is_irrefutable;
use validate_match_arms::validate_match_arms;
use validate_pattern::validate_pattern;
