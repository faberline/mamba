use super::*;

/// Compound statement parsers: def, class, enum, if, while, for, match,
/// try, raise, with, assert, del, global, nonlocal, type alias, decorated.
impl<'a> Parser<'a> {
    pub(crate) fn parse_raise(&mut self) -> crate::error::Result<Spanned<Stmt>> {
        let (start, _) = self.advance();
        if self.peek_kind() == Some(TokenKind::Newline) || self.peek_kind() == Some(TokenKind::Eof)
        {
            self.skip_newlines();
            return Ok(Spanned::new(
                Stmt::Raise {
                    value: None,
                    from: None,
                },
                self.span_from(start),
            ));
        }
        let value = self.parse_expr()?;
        let from = if self.peek_kind() == Some(TokenKind::From) {
            self.advance();
            Some(self.parse_expr()?)
        } else {
            None
        };
        self.skip_newlines();
        Ok(Spanned::new(
            Stmt::Raise {
                value: Some(value),
                from,
            },
            self.span_from(start),
        ))
    }
}
