use super::*;

impl<'a> Parser<'a> {
    pub(super) fn parse_dotted_value_key(&mut self) -> crate::error::Result<Spanned<Expr>> {
        let start = self.peek().map(|t| t.start).unwrap_or(0);
        let (ns, ne) = self.expect_name()?;
        let mut expr = Spanned::new(
            Expr::Ident(self.text_at(ns, ne).to_string()),
            Span::new(self.file_id, ns, ne),
        );
        let mut saw_dot = false;
        while self.peek_kind() == Some(TokenKind::Dot) {
            saw_dot = true;
            self.advance();
            let (as_, ae) = self.expect_name()?;
            let attr = self.text_at(as_, ae).to_string();
            let span = expr.span.merge(Span::new(self.file_id, as_, ae));
            expr = Spanned::new(
                Expr::Attr {
                    object: Box::new(expr),
                    attr,
                },
                span,
            );
        }
        if !saw_dot {
            return Err(MambaError::syntax(
                self.span_from(start),
                "mapping pattern key must be a literal or dotted value",
            ));
        }
        Ok(expr)
    }
}
