use crate::ast::*;
use crate::source::span::Spanned;

mod collect_assignment_targets;
mod collect_expr_targets;
mod collect_pattern_targets;
mod collect_walrus_targets;
mod collect_walrus_targets_in_stmts;

pub use collect_assignment_targets::collect_assignment_targets;
use collect_expr_targets::collect_expr_targets;
use collect_pattern_targets::collect_pattern_targets;
pub use collect_walrus_targets::collect_walrus_targets;
pub use collect_walrus_targets_in_stmts::collect_walrus_targets_in_stmts;
