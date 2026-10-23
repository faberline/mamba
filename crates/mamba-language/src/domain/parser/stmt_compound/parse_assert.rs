use super::*;

/// Compound statement parsers: def, class, enum, if, while, for, match,
/// try, raise, with, assert, del, global, nonlocal, type alias, decorated.
impl<'a> Parser<'a> {
    pub(crate) fn parse_assert(&mut self) -> crate::error::Result<Spanned<Stmt>> {
        let (start, _) = self.advance();
        let test = self.parse_expr()?;
        let msg = if self.peek_kind() == Some(TokenKind::Comma) {
            self.advance();
            Some(self.parse_expr()?)
        } else {
            None
        };
        self.skip_newlines();
        Ok(Spanned::new(
            Stmt::Assert { test, msg },
            self.span_from(start),
        ))
    }
}
