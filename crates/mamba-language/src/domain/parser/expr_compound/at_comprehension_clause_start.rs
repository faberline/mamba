use super::*;

impl<'a> Parser<'a> {
    pub(crate) fn at_comprehension_clause_start(&self) -> bool {
        self.peek_kind() == Some(TokenKind::For)
            || (self.peek_kind() == Some(TokenKind::Async)
                && self.peek_at(1).is_some_and(|kind| *kind == TokenKind::For))
    }
}
