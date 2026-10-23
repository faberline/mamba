use super::ast::*;
use super::Parser;
use crate::error::MambaError;
use crate::source::span::{Span, Spanned};
use mamba_shared_kernel::token::{TokenKind, BIG_INT_LITERAL_SENTINEL};

#[cfg(test)]
mod tests;

mod parse_builtin_type_pattern;
mod parse_class_or_constructor_args;
mod parse_dotted_value_key;
mod parse_ident_pattern;
mod parse_literal_atom;
mod parse_mapping_key;
mod parse_mapping_pattern;
mod parse_paren_sequence_pattern;
mod parse_pattern;
mod parse_pattern_literal;
mod parse_sequence_element_pattern;
mod parse_sequence_pattern;
mod parse_single_pattern;
