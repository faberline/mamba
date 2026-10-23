use super::*;

impl<'a> Parser<'a> {
    pub(super) fn token_to_binop(&self, kind: &TokenKind) -> Option<BinOp> {
        match kind {
            TokenKind::Plus => Some(BinOp::Add),
            TokenKind::Minus => Some(BinOp::Sub),
            TokenKind::Star => Some(BinOp::Mul),
            TokenKind::Slash => Some(BinOp::Div),
            TokenKind::DoubleSlash => Some(BinOp::FloorDiv),
            TokenKind::Percent => Some(BinOp::Mod),
            TokenKind::DoubleStar => Some(BinOp::Pow),
            TokenKind::At => Some(BinOp::MatMul),
            TokenKind::EqEq => Some(BinOp::Eq),
            TokenKind::NotEq => Some(BinOp::NotEq),
            TokenKind::Lt => Some(BinOp::Lt),
            TokenKind::Gt => Some(BinOp::Gt),
            TokenKind::LtEq => Some(BinOp::LtEq),
            TokenKind::GtEq => Some(BinOp::GtEq),
            TokenKind::And => Some(BinOp::And),
            TokenKind::Or => Some(BinOp::Or),
            TokenKind::Amp => Some(BinOp::BitAnd),
            TokenKind::Pipe => Some(BinOp::BitOr),
            TokenKind::Caret => Some(BinOp::BitXor),
            TokenKind::LShift => Some(BinOp::LShift),
            TokenKind::RShift => Some(BinOp::RShift),
            TokenKind::In => Some(BinOp::In),
            TokenKind::Is => {
                if self.peek_at(1) == Some(&TokenKind::Not) {
                    Some(BinOp::IsNot)
                } else {
                    Some(BinOp::Is)
                }
            }
            TokenKind::Not => {
                if self.peek_at(1) == Some(&TokenKind::In) {
                    Some(BinOp::NotIn)
                } else {
                    None
                }
            }
            _ => None,
        }
    }
}
