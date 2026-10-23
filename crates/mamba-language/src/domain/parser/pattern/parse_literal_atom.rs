use super::*;

/// Parse a single literal atom: Int, Float, Str, True, False, None_.
impl<'a> Parser<'a> {
    pub(super) fn parse_literal_atom(&mut self) -> crate::error::Result<Spanned<Expr>> {
        let token = self
            .peek()
            .ok_or_else(|| MambaError::syntax(Span::dummy(), "expected literal"))?;
        let start = token.start;
        match &token.kind {
            TokenKind::Int(v) => {
                let v = *v;
                let (_tok_start, tok_end) = self.advance();
                let span = self.span_from(start);
                if v == BIG_INT_LITERAL_SENTINEL {
                    Ok(Spanned::new(
                        Expr::BigIntLit(self.text_at(start, tok_end).to_string()),
                        span,
                    ))
                } else {
                    Ok(Spanned::new(Expr::IntLit(v), span))
                }
            }
            TokenKind::Float(v) => {
                let v = *v;
                self.advance();
                Ok(Spanned::new(Expr::FloatLit(v), self.span_from(start)))
            }
            TokenKind::Str(v) | TokenKind::TripleStr(v) | TokenKind::RawStr(v) => {
                let v = v.clone();
                self.advance();
                Ok(Spanned::new(Expr::StrLit(v), self.span_from(start)))
            }
            TokenKind::True => {
                self.advance();
                Ok(Spanned::new(Expr::BoolLit(true), self.span_from(start)))
            }
            TokenKind::False => {
                self.advance();
                Ok(Spanned::new(Expr::BoolLit(false), self.span_from(start)))
            }
            TokenKind::None_ => {
                self.advance();
                Ok(Spanned::new(Expr::NoneLit, self.span_from(start)))
            }
            other => Err(MambaError::syntax(
                Span::new(self.file_id, start, token.end),
                format!("expected literal, got {other}"),
            )),
        }
    }
}
