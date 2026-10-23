use super::*;

impl<'a> Parser<'a> {
    pub(super) fn parse_import(&mut self) -> crate::error::Result<Spanned<Stmt>> {
        let (start, _) = self.advance();
        let (s, e) = self.expect_name()?;
        let mut module = vec![self.text_at(s, e).to_string()];
        while self.peek_kind() == Some(TokenKind::Dot) {
            self.advance();
            let (s, e) = self.expect_name()?;
            module.push(self.text_at(s, e).to_string());
        }
        // `import X as Y` — module-level alias (#1014)
        let module_alias = if self.peek_kind() == Some(TokenKind::As) {
            self.advance();
            let (a_s, a_e) = self.expect_name()?;
            Some(self.text_at(a_s, a_e).to_string())
        } else {
            None
        };
        // `import a, b as y, c.d` — each comma-separated module is its own import
        // (Python semantics: `import a, b` == `import a; import b`). Emit the first
        // below; queue the rest via pending_stmts so every alias binds (#1014 fix).
        let mut extra: Vec<Spanned<Stmt>> = Vec::new();
        while self.peek_kind() == Some(TokenKind::Comma) {
            self.advance(); // consume `,`
            let (s, e) = self.expect_name()?;
            let mut m = vec![self.text_at(s, e).to_string()];
            while self.peek_kind() == Some(TokenKind::Dot) {
                self.advance();
                let (s, e) = self.expect_name()?;
                m.push(self.text_at(s, e).to_string());
            }
            let m_alias = if self.peek_kind() == Some(TokenKind::As) {
                self.advance();
                let (a_s, a_e) = self.expect_name()?;
                Some(self.text_at(a_s, a_e).to_string())
            } else {
                None
            };
            extra.push(Spanned::new(
                Stmt::Import {
                    module: m,
                    names: None,
                    module_alias: m_alias,
                },
                self.span_from(start),
            ));
        }
        self.skip_newlines();
        let first = Spanned::new(
            Stmt::Import {
                module,
                names: None,
                module_alias,
            },
            self.span_from(start),
        );
        if !extra.is_empty() {
            // pending_stmts is drained LIFO (pop), so reverse to keep source order.
            extra.reverse();
            self.pending_stmts.extend(extra);
        }
        Ok(first)
    }
}
