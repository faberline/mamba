use super::*;

/// Compound statement parsers: def, class, enum, if, while, for, match,
/// try, raise, with, assert, del, global, nonlocal, type alias, decorated.
impl<'a> Parser<'a> {
    pub(crate) fn parse_match(&mut self) -> crate::error::Result<Spanned<Stmt>> {
        let (start, _) = self.advance();
        let expr = self.parse_expr()?;
        self.expect(TokenKind::Colon)?;
        self.skip_newlines();
        self.expect(TokenKind::Indent)?;
        let mut arms = Vec::new();
        while self.peek_kind() != Some(TokenKind::Dedent)
            && self.peek_kind() != Some(TokenKind::Eof)
        {
            self.skip_newlines();
            if self.peek_kind() == Some(TokenKind::Dedent) {
                break;
            }
            let arm_start = self.peek().map(|t| t.start).unwrap_or(0);
            self.expect(TokenKind::Case)?;
            let pattern = self.parse_pattern()?;
            let guard = if self.peek_kind() == Some(TokenKind::If) {
                self.advance();
                Some(self.parse_expr()?)
            } else {
                None
            };
            self.expect(TokenKind::Colon)?;
            let body = self.parse_block()?;
            arms.push(MatchArm {
                pattern,
                guard,
                body,
                span: self.span_from(arm_start),
            });
        }
        if self.peek_kind() == Some(TokenKind::Dedent) {
            self.advance();
        }
        if let Err(msg) = validate_match_arms(&arms) {
            return Err(crate::error::MambaError::syntax(self.span_from(start), msg));
        }
        Ok(Spanned::new(
            Stmt::Match { expr, arms },
            self.span_from(start),
        ))
    }
}
