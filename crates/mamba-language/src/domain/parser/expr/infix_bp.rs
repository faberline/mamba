use super::*;

pub(super) fn infix_bp(op: &BinOp) -> (u8, u8) {
    match op {
        BinOp::Or => (1, 2),
        BinOp::And => (3, 4),
        BinOp::Eq
        | BinOp::NotEq
        | BinOp::Lt
        | BinOp::Gt
        | BinOp::LtEq
        | BinOp::GtEq
        | BinOp::Is
        | BinOp::IsNot
        | BinOp::In
        | BinOp::NotIn => (7, 8),
        BinOp::BitOr => (9, 10),
        BinOp::BitXor => (11, 12),
        BinOp::BitAnd => (13, 14),
        BinOp::LShift | BinOp::RShift => (15, 16),
        BinOp::Add | BinOp::Sub => (17, 18),
        BinOp::Mul | BinOp::Div | BinOp::FloorDiv | BinOp::Mod | BinOp::MatMul => (19, 20),
        BinOp::Pow => (24, 23), // Right-associative
    }
}
