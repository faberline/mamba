use super::*;

/// Compound statement parsers: def, class, enum, if, while, for, match,
/// try, raise, with, assert, del, global, nonlocal, type alias, decorated.
impl<'a> Parser<'a> {
    pub(crate) fn parse_class_def(
        &mut self,
        decorators: Vec<Spanned<Expr>>,
    ) -> crate::error::Result<Spanned<Stmt>> {
        let (start, _) = self.advance(); // consume `class`
        let (ns, ne) = self.expect_name()?;
        let name = self.text_at(ns, ne).to_string();
        let type_params = self.parse_optional_type_params()?;
        let (bases, keyword_args) = if self.peek_kind() == Some(TokenKind::LParen) {
            self.advance();
            let mut bases = Vec::new();
            let mut keyword_args = Vec::new();
            while self.peek_kind() != Some(TokenKind::RParen)
                && self.peek_kind() != Some(TokenKind::Eof)
            {
                // CPython grammar permits the full call-argument shape in
                // a class header — positional, keyword, `*expr`, `**expr`
                // (#1674). HIR lowering's `filter_map` over bases /
                // `find_map` over keyword_args drops these silently, so
                // accepting the syntax unblocks module-load even before
                // proper runtime semantics for class-base unpacking
                // lands (a separate enhancement).
                match self.peek_kind() {
                    Some(TokenKind::DoubleStar) => {
                        self.advance(); // consume `**`
                        let value = self.parse_expr()?;
                        keyword_args.push(("**".to_string(), value));
                    }
                    Some(TokenKind::Star) => {
                        let (sp_start, _) = self.advance(); // consume `*`
                        let inner = self.parse_expr()?;
                        let span = self.span_from(sp_start);
                        bases.push(Spanned::new(Expr::Starred(Box::new(inner)), span));
                    }
                    _ => {
                        let expr = self.parse_expr()?;
                        // Handle keyword arg: `metaclass=Metaclass`, `name="alpha"`
                        if self.peek_kind() == Some(TokenKind::Eq) {
                            self.advance(); // consume =
                            let value = self.parse_expr()?;
                            if let Expr::Ident(key_name) = &expr.node {
                                keyword_args.push((key_name.clone(), value));
                            }
                        } else {
                            bases.push(expr);
                        }
                    }
                }
                if self.peek_kind() == Some(TokenKind::Comma) {
                    self.advance();
                }
            }
            self.expect(TokenKind::RParen)?;
            (bases, keyword_args)
        } else {
            (Vec::new(), Vec::new())
        };
        if !type_params.is_empty() {
            self.validate_pep695_generic_exprs(&bases)?;
            for (_, value) in &keyword_args {
                self.validate_pep695_expr(
                    value,
                    super::super::stmt::Pep695ExprContext::GenericDefinition,
                )?;
            }
        }
        self.expect(TokenKind::Colon)?;
        let was_in_class_body = self.in_class_body;
        self.in_class_body = true;
        let body = self.parse_block();
        self.in_class_body = was_in_class_body;
        let body = body?;
        Ok(Spanned::new(
            Stmt::ClassDef {
                decorators,
                name,
                type_params,
                bases,
                keyword_args,
                body,
            },
            self.span_from(start),
        ))
    }
}
