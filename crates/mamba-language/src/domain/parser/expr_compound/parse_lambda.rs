use super::*;

/// Lambda: `lambda x, y: body` (Python-style) or `lambda x: int: body` (typed).
/// Type annotations are optional — omitted params default to Any.
impl<'a> Parser<'a> {
    pub(crate) fn parse_lambda(&mut self) -> crate::error::Result<Spanned<Expr>> {
        let (start, _) = self.advance(); // consume `lambda`
        let mut params: Vec<Param> = Vec::new();

        // No-param lambda: `lambda: body`
        // PEP 570/3102 markers are valid in lambda parameter lists too:
        // a `/` retroactively marks prior params positional-only, and a bare
        // `*` (or `*args`) makes the following params keyword-only.
        let mut seen_star = false;
        if self.peek_kind() != Some(TokenKind::Colon) {
            loop {
                let p_start = self.peek().map(|t| t.start).unwrap_or(0);

                // `/` positional-only separator (`lambda a, /, b: ...`).
                if self.peek_kind() == Some(TokenKind::Slash) {
                    self.advance();
                    for p in params.iter_mut() {
                        if p.kind == ParamKind::Regular {
                            p.pos_only = true;
                        }
                    }
                    if self.peek_kind() == Some(TokenKind::Comma) {
                        self.advance();
                        continue;
                    }
                    break;
                }

                // Handle **kwargs / *args / bare `*`
                let kind = if self.peek_kind() == Some(TokenKind::DoubleStar) {
                    self.advance();
                    ParamKind::DoubleStar
                } else if self.peek_kind() == Some(TokenKind::Star) {
                    self.advance();
                    // bare `*` (keyword-only separator, `lambda a, *, c: ...`)
                    // vs `*args`: a name must follow for the latter.
                    if !self.peek_kind().as_ref().is_some_and(Self::is_name_token) {
                        seen_star = true;
                        if self.peek_kind() == Some(TokenKind::Comma) {
                            self.advance();
                            continue;
                        }
                        break;
                    }
                    seen_star = true;
                    ParamKind::Star
                } else {
                    ParamKind::Regular
                };

                let (ns, ne) = self.expect_name()?;
                let name = self.text_at(ns, ne).to_string();

                // Optional type annotation: disambiguate `x: type` vs `x: body`.
                // `,` alone after the tentative type is NOT enough to confirm
                // (#1582) — `lambda x: x, R(...)` inside a call-arg list is a
                // valid Python idiom where the body is `x` and `, R(...)` is
                // the next call arg. Only confirm on `:` / `=`, or on `,`
                // followed by a depth-0 body-`:` further ahead (which marks
                // a real typed-lambda continuation `lambda x: T, y: U: body`).
                let (ty, annotation) = if kind == ParamKind::Regular
                    && self.peek_kind() == Some(TokenKind::Colon)
                {
                    let saved = self.pos;
                    self.advance(); // tentatively consume :
                    if let Ok(type_expr) = self.parse_type_expr() {
                        let next = self.peek_kind();
                        let confirmed = match next {
                            Some(TokenKind::Colon) | Some(TokenKind::Eq) => true,
                            Some(TokenKind::Comma) => self.has_lambda_body_colon_ahead(),
                            _ => false,
                        };
                        if confirmed {
                            (type_expr.clone(), ParamAnnotation::Authored(type_expr))
                        } else {
                            self.pos = saved; // backtrack — : is body separator
                            (
                                Spanned::new(
                                    TypeExpr::Named("Any".into()),
                                    self.span_from(p_start),
                                ),
                                ParamAnnotation::Omitted,
                            )
                        }
                    } else {
                        self.pos = saved; // backtrack
                        (
                            Spanned::new(TypeExpr::Named("Any".into()), self.span_from(p_start)),
                            ParamAnnotation::Omitted,
                        )
                    }
                } else {
                    (
                        Spanned::new(TypeExpr::Named("Any".into()), self.span_from(p_start)),
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
                    kind,
                    pos_only: false,
                    kw_only: seen_star && kind == ParamKind::Regular,
                    span: self.span_from(p_start),
                });

                if self.peek_kind() == Some(TokenKind::Comma) {
                    self.advance();
                } else {
                    break;
                }
            }
        }

        self.expect(TokenKind::Colon)?;
        if self.peek_kind().as_ref().is_some_and(Self::is_name_token)
            && self.peek_at(1).is_some_and(|k| *k == TokenKind::ColonEq)
        {
            return Err(crate::error::MambaError::syntax(
                self.peek()
                    .map(|t| Span::new(self.file_id, t.start, t.end))
                    .unwrap_or_else(|| self.span_from(start)),
                "invalid syntax",
            ));
        }
        let was_in_class_body = self.in_class_body;
        self.in_class_body = false;
        let body = self.parse_expr();
        self.in_class_body = was_in_class_body;
        let body = body?;
        let span = Span::new(self.file_id, start, body.span.end);
        Ok(Spanned::new(
            Expr::Lambda {
                params,
                body: Box::new(body),
            },
            span,
        ))
    }
}
