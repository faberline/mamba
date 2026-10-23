use super::*;

/// Compound statement parsers: def, class, enum, if, while, for, match,
/// try, raise, with, assert, del, global, nonlocal, type alias, decorated.
impl<'a> Parser<'a> {
    pub(crate) fn parse_del(&mut self) -> crate::error::Result<Spanned<Stmt>> {
        let (start, _) = self.advance();
        // Support `del a, b, c` — comma-separated delete targets.
        let target = self.parse_tuple_or_expr()?;
        self.skip_newlines();
        Ok(Spanned::new(Stmt::Del(target), self.span_from(start)))
    }
}
