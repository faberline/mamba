use super::*;

impl<'a> Parser<'a> {
    pub(super) fn advance_binop(&mut self, op: &BinOp) {
        match op {
            BinOp::IsNot | BinOp::NotIn => {
                self.advance();
                self.advance();
            }
            _ => {
                self.advance();
            }
        }
    }
}
