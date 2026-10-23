use super::ast::*;
use super::Parser;
use crate::error::MambaError;
use crate::source::span::{Span, Spanned};
use mamba_shared_kernel::token::{TokenKind, BIG_INT_LITERAL_SENTINEL};

#[cfg(test)]
mod tests;

mod advance_binop;
mod finish_fstring_literal;
mod finish_slice_element;
mod infix_bp;
mod is_comparison_op;
mod parse_attr;
mod parse_call;
mod parse_expr;
mod parse_expr_bp;
mod parse_fstring_expr;
#[cfg(test)]
mod parse_fstring_parts;
mod parse_fstring_parts_with_lexer;
mod parse_index_or_slice;
mod parse_prefix;
mod parse_subscript_element;
mod parse_tuple_or_expr;
mod peek_at;
mod prefix_bp;
mod split_expr_and_conversion;
mod split_expr_and_spec;
mod strip_fstring_literal;
mod token_to_binop;

use finish_fstring_literal::finish_fstring_literal;
use infix_bp::infix_bp;
use is_comparison_op::is_comparison_op;
use parse_fstring_expr::parse_fstring_expr;
#[cfg(test)]
use parse_fstring_parts::parse_fstring_parts;
use parse_fstring_parts_with_lexer::parse_fstring_parts_with_lexer;
pub(crate) use prefix_bp::prefix_bp;
use split_expr_and_conversion::split_expr_and_conversion;
use split_expr_and_spec::split_expr_and_spec;
use strip_fstring_literal::strip_fstring_literal;
