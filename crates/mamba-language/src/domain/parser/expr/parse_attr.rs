use super::*;

impl<'a> Parser<'a> {
    pub(super) fn parse_attr(
        &mut self,
        object: Spanned<Expr>,
    ) -> crate::error::Result<Spanned<Expr>> {
        self.expect(TokenKind::Dot)?;
        let (ns, ne) = self.expect_name()?;
        let attr = self.text_at(ns, ne).to_string();
        let span = object.span.merge(Span::new(self.file_id, ns, ne));
        Ok(Spanned::new(
            Expr::Attr {
                object: Box::new(object),
                attr,
            },
            span,
        ))
    }
}
