use super::*;

/// Parse one or more comma-separated expressions into a tuple or single expr.
/// Used for bare tuple on the RHS of assignment: `x = a, b, c`.
impl<'a> Parser<'a> {
    pub fn parse_tuple_or_expr(&mut self) -> crate::error::Result<Spanned<Expr>> {
        let first = self.parse_expr()?;
        if self.peek_kind() != Some(TokenKind::Comma) {
            return Ok(first);
        }
        let mut elems = vec![first];
        while self.peek_kind() == Some(TokenKind::Comma) {
            self.advance();
            // Trailing comma: `x = a,` or end of statement
            if self.peek_kind() == Some(TokenKind::Newline)
                || self.peek_kind() == Some(TokenKind::Eof)
                || self.peek_kind() == Some(TokenKind::Dedent)
            {
                break;
            }
            elems.push(self.parse_expr()?);
        }
        let span = elems
            .first()
            .unwrap()
            .span
            .merge(elems.last().unwrap().span);
        Ok(Spanned::new(Expr::TupleLit(elems), span))
    }
}
