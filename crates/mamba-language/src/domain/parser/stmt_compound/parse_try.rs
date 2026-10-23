use super::*;

/// Compound statement parsers: def, class, enum, if, while, for, match,
/// try, raise, with, assert, del, global, nonlocal, type alias, decorated.
impl<'a> Parser<'a> {
    pub(crate) fn parse_try(&mut self) -> crate::error::Result<Spanned<Stmt>> {
        let (start, _) = self.advance(); // consume `try`
        self.expect(TokenKind::Colon)?;
        let body = self.parse_block()?;
        let mut handlers = Vec::new();
        while self.peek_kind() == Some(TokenKind::Except) {
            let h_start = self.peek().unwrap().start;
            self.advance(); // consume `except`
                            // Check for `except*` (PEP 654)
            let is_star = if self.peek_kind() == Some(TokenKind::Star) {
                self.advance(); // consume `*`
                true
            } else {
                false
            };
            let (exc_type, name) = if self.peek_kind() != Some(TokenKind::Colon) {
                let exc = self.parse_expr()?;
                let name = if self.peek_kind() == Some(TokenKind::As) {
                    self.advance();
                    let (ns, ne) = self.expect_name()?;
                    Some(self.text_at(ns, ne).to_string())
                } else {
                    None
                };
                (Some(exc), name)
            } else {
                (None, None)
            };
            self.expect(TokenKind::Colon)?;
            let h_body = self.parse_block()?;
            handlers.push(ExceptHandler {
                exc_type,
                name,
                body: h_body,
                is_star,
                span: self.span_from(h_start),
            });
        }
        let else_body = if self.peek_kind() == Some(TokenKind::Else) {
            self.advance();
            self.expect(TokenKind::Colon)?;
            Some(self.parse_block()?)
        } else {
            None
        };
        let finally_body = if self.peek_kind() == Some(TokenKind::Finally) {
            self.advance();
            self.expect(TokenKind::Colon)?;
            Some(self.parse_block()?)
        } else {
            None
        };
        Ok(Spanned::new(
            Stmt::Try {
                body,
                handlers,
                else_body,
                finally_body,
            },
            self.span_from(start),
        ))
    }
}
