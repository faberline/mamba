use super::*;

impl<'a> Parser<'a> {
    pub(super) fn parse_return(&mut self) -> crate::error::Result<Spanned<Stmt>> {
        let (start, _) = self.advance();
        let value = if self.peek_kind() != Some(TokenKind::Newline)
            && self.peek_kind() != Some(TokenKind::Dedent)
            && self.peek_kind() != Some(TokenKind::Eof)
            && self.peek_kind() != Some(TokenKind::Semicolon)
        {
            Some(self.parse_tuple_or_expr()?)
        } else {
            None
        };
        self.skip_newlines();
        Ok(Spanned::new(Stmt::Return(value), self.span_from(start)))
    }
}
