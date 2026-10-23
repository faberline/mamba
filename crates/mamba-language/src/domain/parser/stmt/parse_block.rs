use super::*;

/// Parse an indented block or single-line suite.
/// Python allows: `if cond: stmt` (single-line) or `if cond:\n    stmt` (block).
impl<'a> Parser<'a> {
    pub fn parse_block(&mut self) -> crate::error::Result<Vec<Spanned<Stmt>>> {
        // Single-line suite: no newline/indent after colon
        if self.peek_kind() != Some(TokenKind::Newline)
            && self.peek_kind() != Some(TokenKind::Indent)
        {
            let stmt = self.parse_stmt()?;
            let mut stmts = vec![stmt];
            while !self.pending_stmts.is_empty() {
                stmts.push(self.parse_stmt()?);
            }
            // Handle semicolons in single-line suite
            while self.peek_kind() == Some(TokenKind::Semicolon) {
                self.advance();
                while self.peek_kind() == Some(TokenKind::Semicolon) {
                    self.advance();
                }
                if self.peek_kind() == Some(TokenKind::Newline)
                    || self.peek_kind() == Some(TokenKind::Eof)
                    || self.peek_kind().is_none()
                {
                    break;
                }
                stmts.push(self.parse_stmt()?);
                while !self.pending_stmts.is_empty() {
                    stmts.push(self.parse_stmt()?);
                }
            }
            return Ok(stmts);
        }

        self.skip_newlines();
        self.expect(TokenKind::Indent)?;
        let mut stmts = Vec::new();
        while !self.pending_stmts.is_empty()
            || (self.peek_kind() != Some(TokenKind::Dedent)
                && self.peek_kind() != Some(TokenKind::Eof))
        {
            // Drain any desugared continuations (e.g. chained assign tail) that
            // belong to the *current* block before deciding whether to stop.
            if self.pending_stmts.is_empty() {
                self.skip_newlines();
                if self.peek_kind() == Some(TokenKind::Dedent)
                    || self.peek_kind() == Some(TokenKind::Eof)
                {
                    break;
                }
            }
            stmts.push(self.parse_stmt()?);
            // Handle semicolons inside indented blocks
            while self.peek_kind() == Some(TokenKind::Semicolon) {
                self.advance();
                while self.peek_kind() == Some(TokenKind::Semicolon) {
                    self.advance();
                }
                if self.peek_kind() == Some(TokenKind::Newline)
                    || self.peek_kind() == Some(TokenKind::Dedent)
                    || self.peek_kind() == Some(TokenKind::Eof)
                    || self.peek_kind().is_none()
                {
                    break;
                }
                stmts.push(self.parse_stmt()?);
            }
        }
        if self.peek_kind() == Some(TokenKind::Dedent) {
            self.advance();
        }
        Ok(stmts)
    }
}
