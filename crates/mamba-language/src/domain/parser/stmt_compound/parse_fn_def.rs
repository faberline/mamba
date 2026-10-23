use super::*;

/// Compound statement parsers: def, class, enum, if, while, for, match,
/// try, raise, with, assert, del, global, nonlocal, type alias, decorated.
impl<'a> Parser<'a> {
    pub(crate) fn parse_fn_def(
        &mut self,
        decorators: Vec<Spanned<Expr>>,
    ) -> crate::error::Result<Spanned<Stmt>> {
        let (start, _) = self.advance(); // consume `def`
        let (ns, ne) = self.expect_name()?;
        let name = self.text_at(ns, ne).to_string();
        let type_params = self.parse_optional_type_params()?;
        self.expect(TokenKind::LParen)?;
        let params = self.parse_params()?;
        self.expect(TokenKind::RParen)?;
        let return_ty = if self.peek_kind() == Some(TokenKind::Arrow) {
            self.advance();
            Some(self.parse_type_expr()?)
        } else {
            None
        };
        self.expect(TokenKind::Colon)?;
        let was_in_class_body = self.in_class_body;
        self.in_class_body = false;
        let body = self.parse_block();
        self.in_class_body = was_in_class_body;
        let body = body?;
        Ok(Spanned::new(
            Stmt::FnDef {
                decorators,
                name,
                type_params,
                params,
                return_ty,
                body,
            },
            self.span_from(start),
        ))
    }
}
