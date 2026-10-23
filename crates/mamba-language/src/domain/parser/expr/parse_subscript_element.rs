use super::*;

/// Parse one subscript element — either an `Expr::Slice` if a colon
/// is present, or a regular expression. Stops at `,` and `]` so the
/// caller can decide whether to wrap a sequence in a tuple.
impl<'a> Parser<'a> {
    pub(super) fn parse_subscript_element(&mut self) -> crate::error::Result<Spanned<Expr>> {
        if self.peek_kind() == Some(TokenKind::Colon) {
            return self.finish_slice_element(None);
        }
        let first = self.parse_expr()?;
        if self.peek_kind() == Some(TokenKind::Colon) {
            return self.finish_slice_element(Some(first));
        }
        Ok(first)
    }
}
