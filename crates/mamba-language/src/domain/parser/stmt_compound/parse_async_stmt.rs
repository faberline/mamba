use super::*;

/// Compound statement parsers: def, class, enum, if, while, for, match,
/// try, raise, with, assert, del, global, nonlocal, type alias, decorated.
impl<'a> Parser<'a> {
    pub(crate) fn parse_async_stmt(&mut self) -> crate::error::Result<Spanned<Stmt>> {
        let start = self.peek().unwrap().start;
        self.advance(); // consume `async`
        match self.peek_kind() {
            Some(TokenKind::Def) => self.parse_async_fn_def(vec![], start),
            Some(TokenKind::For) => self.parse_for(true),
            Some(TokenKind::With) => self.parse_with(true),
            _ => Err(MambaError::syntax(
                self.span_from(start),
                "expected 'def', 'for', or 'with' after 'async'",
            )),
        }
    }
}
