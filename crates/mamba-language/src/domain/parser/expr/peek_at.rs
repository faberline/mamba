use super::*;

// --- Lookahead helpers for multi-token operators ---

impl<'a> Parser<'a> {
    pub(crate) fn peek_at(&self, offset: usize) -> Option<&TokenKind> {
        self.tokens.get(self.pos + offset).map(|t| &t.kind)
    }
}
