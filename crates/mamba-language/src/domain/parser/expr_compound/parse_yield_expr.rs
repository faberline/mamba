use super::*;

/// Yield: `yield expr` or `yield from expr`
impl<'a> Parser<'a> {
    pub(crate) fn parse_yield_expr(&mut self) -> crate::error::Result<Spanned<Expr>> {
        let (start, _) = self.advance(); // consume `yield`
        if self.peek_kind() == Some(TokenKind::From) {
            self.advance(); // consume `from`
            let expr = self.parse_expr()?;
            let span = Span::new(self.file_id, start, expr.span.end);
            return Ok(Spanned::new(Expr::YieldFrom(Box::new(expr)), span));
        }
        if self.peek_kind() == Some(TokenKind::Newline)
            || self.peek_kind() == Some(TokenKind::Dedent)
            || self.peek_kind() == Some(TokenKind::Eof)
            || self.peek_kind() == Some(TokenKind::RParen)
        {
            return Ok(Spanned::new(Expr::Yield(None), self.span_from(start)));
        }
        let expr = self.parse_expr()?;
        let span = Span::new(self.file_id, start, expr.span.end);
        Ok(Spanned::new(Expr::Yield(Some(Box::new(expr))), span))
    }
}
