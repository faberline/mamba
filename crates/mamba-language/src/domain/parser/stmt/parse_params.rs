use super::*;

// --- Helpers ---

impl<'a> Parser<'a> {
    pub(crate) fn parse_params(&mut self) -> crate::error::Result<Vec<Param>> {
        let mut params = Vec::new();
        // PEP 570/3102 introspection markers: after a bare `*` or `*args`,
        // every regular param is keyword-only; a `/` retroactively marks all
        // params parsed so far as positional-only.
        let mut seen_star = false;
        while self.peek_kind() != Some(TokenKind::RParen)
            && self.peek_kind() != Some(TokenKind::Eof)
        {
            let p_start = self.peek().map(|t| t.start).unwrap_or(0);
            // Handle `self` parameter
            if self.peek_kind() == Some(TokenKind::Self_) {
                self.advance();
                let (ty, annotation) = if self.peek_kind() == Some(TokenKind::Colon) {
                    self.advance();
                    let t = self.parse_type_expr()?;
                    (t.clone(), ParamAnnotation::Authored(t))
                } else {
                    (
                        Spanned::new(TypeExpr::Named("Self".to_string()), self.span_from(p_start)),
                        ParamAnnotation::Omitted,
                    )
                };
                let default = if self.peek_kind() == Some(TokenKind::Eq) {
                    self.advance();
                    Some(self.parse_expr()?)
                } else {
                    None
                };
                params.push(Param {
                    name: "self".to_string(),
                    ty,
                    annotation,
                    default,
                    kind: ParamKind::Regular,
                    pos_only: false,
                    kw_only: seen_star,
                    span: self.span_from(p_start),
                });
            } else if self.peek_kind() == Some(TokenKind::DoubleStar) {
                // **kwargs
                self.advance();
                let (ns, ne) = self.expect_name()?;
                let name = self.text_at(ns, ne).to_string();
                let (ty, annotation) = if self.peek_kind() == Some(TokenKind::Colon) {
                    self.advance();
                    let t = self.parse_type_expr()?;
                    (t.clone(), ParamAnnotation::Authored(t))
                } else {
                    (
                        Spanned::new(TypeExpr::Named("Any".to_string()), self.span_from(p_start)),
                        ParamAnnotation::Omitted,
                    )
                };
                params.push(Param {
                    name,
                    ty,
                    annotation,
                    default: None,
                    kind: ParamKind::DoubleStar,
                    pos_only: false,
                    kw_only: false,
                    span: self.span_from(p_start),
                });
            } else if self.peek_kind() == Some(TokenKind::Slash) {
                // `/` positional-only separator — everything before it is
                // positional-only (introspection metadata; binding unchanged).
                // CPython requires at least one parameter to precede `/`, so a
                // solo slash (`def h(/)`) or one after only `*`/`**` is a
                // SyntaxError.
                if !params.iter().any(|p| p.kind == ParamKind::Regular) {
                    return Err(crate::error::MambaError::syntax(
                        self.span_from(p_start),
                        "at least one argument must precede /",
                    ));
                }
                self.advance();
                for p in params.iter_mut() {
                    if p.kind == ParamKind::Regular {
                        p.pos_only = true;
                    }
                }
            } else if self.peek_kind() == Some(TokenKind::Star) {
                // bare `*` (keyword-only separator) vs `*args`
                self.advance();
                if !self.peek_kind().as_ref().is_some_and(Self::is_name_token) {
                    // bare `*` — keyword-only separator, no param.
                    // Consume the trailing comma so the loop can continue to
                    // the keyword-only parameters that follow.
                    seen_star = true;
                    if self.peek_kind() == Some(TokenKind::Comma) {
                        self.advance();
                    }
                    continue;
                }
                let (ns, ne) = self.expect_name()?;
                let name = self.text_at(ns, ne).to_string();
                let (ty, annotation) = if self.peek_kind() == Some(TokenKind::Colon) {
                    self.advance();
                    let t = self.parse_type_expr()?;
                    (t.clone(), ParamAnnotation::Authored(t))
                } else {
                    (
                        Spanned::new(TypeExpr::Named("Any".to_string()), self.span_from(p_start)),
                        ParamAnnotation::Omitted,
                    )
                };
                seen_star = true;
                params.push(Param {
                    name,
                    ty,
                    annotation,
                    default: None,
                    kind: ParamKind::Star,
                    pos_only: false,
                    kw_only: false,
                    span: self.span_from(p_start),
                });
            } else if self.peek_kind().as_ref().is_some_and(Self::is_name_token) {
                let (ns, ne) = self.expect_name()?;
                let name = self.text_at(ns, ne).to_string();
                let (ty, annotation) = if self.peek_kind() == Some(TokenKind::Colon) {
                    self.advance();
                    let t = self.parse_type_expr()?;
                    (t.clone(), ParamAnnotation::Authored(t))
                } else {
                    (
                        Spanned::new(TypeExpr::Named("Any".to_string()), self.span_from(p_start)),
                        ParamAnnotation::Omitted,
                    )
                };
                let default = if self.peek_kind() == Some(TokenKind::Eq) {
                    self.advance();
                    Some(self.parse_expr()?)
                } else {
                    None
                };
                params.push(Param {
                    name,
                    ty,
                    annotation,
                    default,
                    kind: ParamKind::Regular,
                    pos_only: false,
                    kw_only: seen_star,
                    span: self.span_from(p_start),
                });
            } else {
                break;
            }
            if self.peek_kind() == Some(TokenKind::Comma) {
                self.advance();
            }
        }
        Ok(params)
    }
}
