use super::*;

/// Compound statement parsers: def, class, enum, if, while, for, match,
/// try, raise, with, assert, del, global, nonlocal, type alias, decorated.
impl<'a> Parser<'a> {
    pub(crate) fn parse_if(&mut self) -> crate::error::Result<Spanned<Stmt>> {
        let (start, _) = self.advance();
        let condition = self.parse_expr()?;
        self.expect(TokenKind::Colon)?;
        let body = self.parse_block()?;
        let mut elif_clauses = Vec::new();
        while self.peek_kind() == Some(TokenKind::Elif) {
            self.advance();
            let cond = self.parse_expr()?;
            self.expect(TokenKind::Colon)?;
            elif_clauses.push((cond, self.parse_block()?));
        }
        let else_body = if self.peek_kind() == Some(TokenKind::Else) {
            self.advance();
            self.expect(TokenKind::Colon)?;
            Some(self.parse_block()?)
        } else {
            None
        };
        Ok(Spanned::new(
            Stmt::If {
                condition,
                body,
                elif_clauses,
                else_body,
            },
            self.span_from(start),
        ))
    }
}
