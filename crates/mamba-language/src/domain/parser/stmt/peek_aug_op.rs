use super::*;

impl<'a> Parser<'a> {
    pub(super) fn peek_aug_op(&self) -> Option<AugOp> {
        match self.peek_kind()? {
            TokenKind::PlusEq => Some(AugOp::Add),
            TokenKind::MinusEq => Some(AugOp::Sub),
            TokenKind::StarEq => Some(AugOp::Mul),
            TokenKind::SlashEq => Some(AugOp::Div),
            TokenKind::DoubleSlashEq => Some(AugOp::FloorDiv),
            TokenKind::PercentEq => Some(AugOp::Mod),
            TokenKind::DoubleStarEq => Some(AugOp::Pow),
            TokenKind::AmpEq => Some(AugOp::BitAnd),
            TokenKind::PipeEq => Some(AugOp::BitOr),
            TokenKind::CaretEq => Some(AugOp::BitXor),
            TokenKind::LShiftEq => Some(AugOp::LShift),
            TokenKind::RShiftEq => Some(AugOp::RShift),
            TokenKind::AtEq => Some(AugOp::MatMul),
            _ => None,
        }
    }
}
