use super::*;

/// Compound statement parsers: def, class, enum, if, while, for, match,
/// try, raise, with, assert, del, global, nonlocal, type alias, decorated.
impl<'a> Parser<'a> {
    pub(crate) fn parse_type_alias(&mut self) -> crate::error::Result<Spanned<Stmt>> {
        let (start, _) = self.advance(); // consume `type`
        let (ns, ne) = self.expect_name()?;
        let name = self.text_at(ns, ne).to_string();
        let type_params = self.parse_optional_type_params()?;
        self.expect(TokenKind::Eq)?;
        // PEP 695: the alias value is an arbitrary expression, evaluated
        // lazily at runtime (e.g. `type Lazy[T] = lambda: T`).
        let value = self.parse_expr()?;
        self.validate_pep695_expr(&value, super::super::stmt::Pep695ExprContext::TypeAlias)?;
        self.skip_newlines();
        Ok(Spanned::new(
            Stmt::TypeAlias {
                name,
                type_params,
                value,
            },
            self.span_from(start),
        ))
    }
}
