use super::ast::*;
use super::Parser;
use crate::error::MambaError;
use crate::source::span::{Span, Spanned};
use mamba_shared_kernel::token::TokenKind;
use std::collections::HashSet;

#[derive(Clone, Copy)]
pub(crate) enum Pep695ExprContext {
    TypeAlias,
    TypeParam,
    GenericDefinition,
}

#[cfg(test)]
mod tests;

mod match_subject_has_colon;
mod normalize_assign_target;
mod parse_block;
mod parse_from_import;
mod parse_ident_stmt;
mod parse_import;
mod parse_optional_type_params;
mod parse_params;
mod parse_return;
mod parse_stmt;
mod peek_aug_op;
mod syntax_message;
mod validate_assign_target;
mod validate_assign_targets;
mod validate_pep695_expr;
mod validate_pep695_fstring_part;
mod validate_pep695_generic_exprs;
