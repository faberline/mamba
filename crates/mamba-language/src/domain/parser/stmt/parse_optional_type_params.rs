use super::*;

impl<'a> Parser<'a> {
    pub(crate) fn parse_optional_type_params(
        &mut self,
    ) -> crate::error::Result<Vec<mamba_shared_kernel::ast::TypeParam>> {
        use mamba_shared_kernel::ast::{TypeParam, TypeParamKind};
        if self.peek_kind() != Some(TokenKind::LBracket) {
            return Ok(Vec::new());
        }
        self.advance();
        let mut params = Vec::new();
        let mut seen_names = HashSet::new();
        let mut saw_default = false;
        while self.peek_kind() != Some(TokenKind::RBracket)
            && self.peek_kind() != Some(TokenKind::Eof)
        {
            let param_start = self.peek().map(|t| t.start).unwrap_or(0);
            let param;
            // ParamSpec: **P
            if self.peek_kind() == Some(TokenKind::DoubleStar) {
                self.advance(); // consume **
                let (s, e) = self.expect_name()?;
                if self.peek_kind() == Some(TokenKind::Colon) {
                    let message = if self.peek_at(1) == Some(&TokenKind::LParen) {
                        "cannot use constraints with ParamSpec"
                    } else {
                        "cannot use bound with ParamSpec"
                    };
                    return Err(crate::error::MambaError::syntax(
                        self.span_from(param_start),
                        message,
                    ));
                }
                param = TypeParam {
                    name: self.text_at(s, e).to_string(),
                    kind: TypeParamKind::ParamSpec,
                    bound: None,
                    constraints: None,
                    default: if self.peek_kind() == Some(TokenKind::Eq) {
                        self.advance();
                        Some(self.parse_expr()?)
                    } else {
                        None
                    },
                };
            }
            // TypeVarTuple: *Ts
            else if self.peek_kind() == Some(TokenKind::Star) {
                self.advance(); // consume *
                let (s, e) = self.expect_name()?;
                if self.peek_kind() == Some(TokenKind::Colon) {
                    let message = if self.peek_at(1) == Some(&TokenKind::LParen) {
                        "cannot use constraints with TypeVarTuple"
                    } else {
                        "cannot use bound with TypeVarTuple"
                    };
                    return Err(crate::error::MambaError::syntax(
                        self.span_from(param_start),
                        message,
                    ));
                }
                param = TypeParam {
                    name: self.text_at(s, e).to_string(),
                    kind: TypeParamKind::TypeVarTuple,
                    bound: None,
                    constraints: None,
                    default: if self.peek_kind() == Some(TokenKind::Eq) {
                        self.advance();
                        Some(self.parse_expr()?)
                    } else {
                        None
                    },
                };
            }
            // Regular type param: T  or  T: bound  or  T: (c1, c2, ...)
            else {
                let (s, e) = self.expect_name()?;
                let name = self.text_at(s, e).to_string();
                let mut bound = None;
                let mut constraints = None;
                let mut default = None;
                // Optional bound: T: expr  or  T: (expr1, expr2, ...)
                if self.peek_kind() == Some(TokenKind::Colon) {
                    self.advance(); // consume :
                    if self.peek_kind() == Some(TokenKind::LParen) {
                        // Constrained: T: (int, float, str)
                        self.advance(); // consume (
                        let mut items = Vec::new();
                        while self.peek_kind() != Some(TokenKind::RParen)
                            && self.peek_kind() != Some(TokenKind::Eof)
                        {
                            items.push(self.parse_expr()?);
                            if self.peek_kind() == Some(TokenKind::Comma) {
                                self.advance();
                            }
                        }
                        self.expect(TokenKind::RParen)?;
                        for item in &items {
                            self.validate_pep695_expr(item, Pep695ExprContext::TypeParam)?;
                        }
                        constraints = Some(items);
                    } else {
                        // Bounded: T: int  (any non-tuple expression)
                        let expr = self.parse_expr()?;
                        self.validate_pep695_expr(&expr, Pep695ExprContext::TypeParam)?;
                        bound = Some(expr);
                    }
                }
                if self.peek_kind() == Some(TokenKind::Eq) {
                    self.advance(); // consume =
                    let expr = self.parse_expr()?;
                    self.validate_pep695_expr(&expr, Pep695ExprContext::TypeParam)?;
                    default = Some(expr);
                }
                param = TypeParam {
                    name,
                    kind: TypeParamKind::TypeVar,
                    bound,
                    constraints,
                    default,
                };
            }
            if saw_default && param.default.is_none() {
                return Err(crate::error::MambaError::syntax(
                    self.span_from(param_start),
                    "non-default type parameter follows default type parameter",
                ));
            }
            if !seen_names.insert(param.name.clone()) {
                return Err(crate::error::MambaError::syntax(
                    self.span_from(param_start),
                    format!("duplicate type parameter '{}'", param.name),
                ));
            }
            saw_default |= param.default.is_some();
            params.push(param);
            if self.peek_kind() == Some(TokenKind::Comma) {
                self.advance();
            }
        }
        self.expect(TokenKind::RBracket)?;
        Ok(params)
    }
}
