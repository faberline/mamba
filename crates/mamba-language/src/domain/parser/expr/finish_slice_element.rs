use super::*;

impl<'a> Parser<'a> {
    pub(super) fn finish_slice_element(
        &mut self,
        start: Option<Spanned<Expr>>,
    ) -> crate::error::Result<Spanned<Expr>> {
        let span_start = start
            .as_ref()
            .map(|s| s.span.start)
            .unwrap_or_else(|| self.tokens[self.pos].start);
        let (_colon_s, colon_e) = self.expect(TokenKind::Colon)?;
        let is_terminator =
            |k: &TokenKind| matches!(k, TokenKind::Colon | TokenKind::Comma | TokenKind::RBracket);
        let stop = if !self.peek_kind().as_ref().is_none_or(is_terminator) {
            Some(Box::new(self.parse_expr()?))
        } else {
            None
        };
        let step = if self.peek_kind() == Some(TokenKind::Colon) {
            self.advance();
            let is_post = |k: &TokenKind| matches!(k, TokenKind::Comma | TokenKind::RBracket);
            if !self.peek_kind().as_ref().is_none_or(is_post) {
                Some(Box::new(self.parse_expr()?))
            } else {
                None
            }
        } else {
            None
        };
        let span_end = self
            .tokens
            .get(self.pos.saturating_sub(1))
            .map(|t| t.end)
            .unwrap_or(colon_e);
        Ok(Spanned::new(
            Expr::Slice {
                start: start.map(Box::new),
                stop,
                step,
            },
            Span::new(self.file_id, span_start, span_end),
        ))
    }
}
