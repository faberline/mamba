use super::*;

/// Binding power for Pratt parsing.
pub(crate) fn prefix_bp(op: &UnaryOp) -> u8 {
    match op {
        UnaryOp::Pos | UnaryOp::Neg | UnaryOp::BitNot => 21,
        UnaryOp::Not => 5,
    }
}
