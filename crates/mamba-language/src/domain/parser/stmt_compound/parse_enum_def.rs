use super::*;

/// Compound statement parsers: def, class, enum, if, while, for, match,
/// try, raise, with, assert, del, global, nonlocal, type alias, decorated.
impl<'a> Parser<'a> {
    pub(crate) fn parse_enum_def(&mut self) -> crate::error::Result<Spanned<Stmt>> {
        let (start, _) = self.advance(); // consume `enum`
        let (ns, ne) = self.expect(TokenKind::Ident)?;
        let name = self.text_at(ns, ne).to_string();
        let type_params = self.parse_optional_type_params()?;
        self.expect(TokenKind::Colon)?;
        self.skip_newlines();
        self.expect(TokenKind::Indent)?;
        let mut variants = Vec::new();
        while self.peek_kind() != Some(TokenKind::Dedent)
            && self.peek_kind() != Some(TokenKind::Eof)
        {
            self.skip_newlines();
            if self.peek_kind() == Some(TokenKind::Dedent) {
                break;
            }
            let v_start = self.peek().map(|t| t.start).unwrap_or(0);
            let (vs, ve) = self.expect(TokenKind::Ident)?;
            let v_name = self.text_at(vs, ve).to_string();
            let fields = if self.peek_kind() == Some(TokenKind::LParen) {
                self.advance();
                let params = self.parse_params()?;
                self.expect(TokenKind::RParen)?;
                params
            } else {
                Vec::new()
            };
            variants.push(Variant {
                name: v_name,
                fields,
                span: self.span_from(v_start),
            });
            self.skip_newlines();
        }
        if self.peek_kind() == Some(TokenKind::Dedent) {
            self.advance();
        }
        Ok(Spanned::new(
            Stmt::EnumDef {
                name,
                type_params,
                variants,
            },
            self.span_from(start),
        ))
    }
}
