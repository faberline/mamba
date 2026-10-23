use super::*;

impl<'a> Parser<'a> {
    pub(super) fn consume_balanced_target_trailer(
        &mut self,
        open: TokenKind,
        close: TokenKind,
    ) -> crate::error::Result<()> {
        self.expect(open)?;
        let mut depth = 1usize;
        while depth > 0 {
            let Some(kind) = self.peek_kind() else {
                return Err(crate::error::MambaError::syntax(
                    Span::dummy(),
                    "unexpected end of comprehension target",
                ));
            };
            match kind {
                TokenKind::LParen | TokenKind::LBracket | TokenKind::LBrace => {
                    depth += 1;
                    self.advance();
                }
                TokenKind::RParen | TokenKind::RBracket | TokenKind::RBrace => {
                    depth -= 1;
                    self.advance();
                }
                TokenKind::Eof => {
                    let span = self
                        .peek()
                        .map(|token| Span::new(self.file_id, token.start, token.end))
                        .unwrap_or_else(Span::dummy);
                    return Err(crate::error::MambaError::syntax(
                        span,
                        format!("expected {close}, got EOF"),
                    ));
                }
                _ => {
                    self.advance();
                }
            }
        }
        Ok(())
    }
}
