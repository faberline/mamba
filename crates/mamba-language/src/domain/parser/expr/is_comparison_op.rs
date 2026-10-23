use super::*;

/// Whether a binary operator is a comparison (eligible for chaining).
pub(super) fn is_comparison_op(op: &BinOp) -> bool {
    matches!(
        op,
        BinOp::Lt
            | BinOp::Gt
            | BinOp::LtEq
            | BinOp::GtEq
            | BinOp::Eq
            | BinOp::NotEq
            | BinOp::Is
            | BinOp::IsNot
            | BinOp::In
            | BinOp::NotIn
    )
}
