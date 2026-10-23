use super::*;

impl<'a> Parser<'a> {
    pub(super) fn parse_index_or_slice(
        &mut self,
        object: Spanned<Expr>,
    ) -> crate::error::Result<Spanned<Expr>> {
        self.expect(TokenKind::LBracket)?;
        let first = self.parse_subscript_element()?;
        // Comma-separated subscript list: `m[a, b, c]` → `m[(a, b, c)]` (#1606),
        // including mixed slice + expr forms `m[a:b, c:d]` / `m[:42, ..., :24:]`
        // (#1670). Python builds an implicit tuple inside the subscript.
        if self.peek_kind() == Some(TokenKind::Comma) {
            let mut elems = vec![first];
            while self.peek_kind() == Some(TokenKind::Comma) {
                self.advance();
                if self.peek_kind() == Some(TokenKind::RBracket) {
                    break;
                }
                elems.push(self.parse_subscript_element()?);
            }
            let (end_s, end_e) = self.expect(TokenKind::RBracket)?;
            let outer = Span::new(self.file_id, end_s, end_e);
            let tuple_span = elems
                .first()
                .unwrap()
                .span
                .merge(elems.last().unwrap().span);
            let index = Spanned::new(Expr::TupleLit(elems), tuple_span);
            let span = object.span.merge(outer);
            return Ok(Spanned::new(
                Expr::Index {
                    object: Box::new(object),
                    index: Box::new(index),
                },
                span,
            ));
        }
        let (end_s, end_e) = self.expect(TokenKind::RBracket)?;
        let span = object.span.merge(Span::new(self.file_id, end_s, end_e));
        Ok(Spanned::new(
            Expr::Index {
                object: Box::new(object),
                index: Box::new(first),
            },
            span,
        ))
    }
}
