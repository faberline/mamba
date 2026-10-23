use super::*;

/// Compound statement parsers: def, class, enum, if, while, for, match,
/// try, raise, with, assert, del, global, nonlocal, type alias, decorated.
/// Parse @decorator(s) followed by def/class.
impl<'a> Parser<'a> {
    pub(crate) fn parse_decorated(&mut self) -> crate::error::Result<Spanned<Stmt>> {
        let start = self.peek().unwrap().start;
        let mut decorators = Vec::new();
        while self.peek_kind() == Some(TokenKind::At) {
            self.advance(); // consume @
            let expr = self.parse_expr()?;
            decorators.push(expr);
            self.skip_newlines();
        }
        match self.peek_kind() {
            Some(TokenKind::Def) => self.parse_fn_def(decorators),
            Some(TokenKind::Async) => {
                self.advance(); // consume async
                self.parse_async_fn_def(decorators, start)
            }
            Some(TokenKind::Class) => self.parse_class_def(decorators),
            _ => Err(MambaError::syntax(
                self.span_from(start),
                "expected 'def', 'async def', or 'class' after decorator",
            )),
        }
    }
}
