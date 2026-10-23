use super::*;

/// Compound statement parsers: def, class, enum, if, while, for, match,
/// try, raise, with, assert, del, global, nonlocal, type alias, decorated.
impl<'a> Parser<'a> {
    pub(crate) fn parse_global(&mut self) -> crate::error::Result<Spanned<Stmt>> {
        let (start, _) = self.advance();
        let mut names = Vec::new();
        loop {
            let (ns, ne) = self.expect_name()?;
            names.push(self.text_at(ns, ne).to_string());
            if self.peek_kind() != Some(TokenKind::Comma) {
                break;
            }
            self.advance();
        }
        self.skip_newlines();
        Ok(Spanned::new(Stmt::Global(names), self.span_from(start)))
    }
}
