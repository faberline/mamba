use super::*;

/// Compound statement parsers: def, class, enum, if, while, for, match,
/// try, raise, with, assert, del, global, nonlocal, type alias, decorated.
impl<'a> Parser<'a> {
    pub(crate) fn parse_with(&mut self, is_async: bool) -> crate::error::Result<Spanned<Stmt>> {
        let (start, _) = self.advance(); // consume `with`

        // PEP 617: parenthesized with-statement `with (ctx1 as a, ctx2 as b):` (#1014)
        let parenthesized =
            self.peek_kind() == Some(TokenKind::LParen) && self.is_parenthesized_with();
        if parenthesized {
            self.advance(); // consume `(`
        }

        let mut items = Vec::new();
        // #1592: parenthesized tuple-targets in `as (a, b)` are desugared
        // by synthesizing a fresh alias and prepending an unpack assign
        // to the body. Collect those prepends here.
        let mut tuple_unpack_prepends: Vec<Spanned<Stmt>> = Vec::new();
        loop {
            // Skip newlines inside parenthesized form
            if parenthesized {
                self.skip_newlines();
            }
            // Stop at `)` in parenthesized form
            if parenthesized && self.peek_kind() == Some(TokenKind::RParen) {
                break;
            }

            let context = self.parse_expr()?;
            let alias = if self.peek_kind() == Some(TokenKind::As) {
                self.advance();
                if self.peek_kind() == Some(TokenKind::LParen) {
                    // `as (t1, t2, ...)` — each element may be any assignable
                    // target (bare name, `obj.attr`, `obj[idx]`, …) per the
                    // CPython grammar. Parse each as a full expression and
                    // desugar to a fresh temp + tuple-unpack assign that we
                    // prepend to the with-body below. Tuple-of-attr targets
                    // (e.g. `as (self.a, self.b)`) lower the same way the
                    // standalone `h.a, h.b = ...` assign does.
                    let (lp_start, _) = self.advance(); // consume `(`
                    let mut tuple_elems: Vec<Spanned<Expr>> = Vec::new();
                    tuple_elems.push(self.parse_expr()?);
                    while self.peek_kind() == Some(TokenKind::Comma) {
                        self.advance();
                        if self.peek_kind() == Some(TokenKind::RParen) {
                            break;
                        }
                        tuple_elems.push(self.parse_expr()?);
                    }
                    self.expect(TokenKind::RParen)?;
                    let tmp = format!("__with_target_{}__", lp_start);
                    let span = Span::dummy();
                    let tuple_target = Spanned::new(Expr::TupleLit(tuple_elems), span);
                    let tmp_ident = Spanned::new(Expr::Ident(tmp.clone()), span);
                    tuple_unpack_prepends.push(Spanned::new(
                        Stmt::Assign {
                            target: tuple_target,
                            value: tmp_ident,
                        },
                        span,
                    ));
                    Some(tmp)
                } else {
                    // Bare-name target (`as foo`) keeps the simple alias path;
                    // attribute / subscript targets (`as foo.bar`, `as foo[i]`)
                    // are valid in CPython and desugar to a fresh temp + a
                    // prepended `target = __tmp__` assignment, matching the
                    // tuple-unpack desugar above.
                    let is_simple_name = match self.peek_kind() {
                        Some(k) if Self::is_name_token(&k) => matches!(
                            self.tokens.get(self.pos + 1).map(|t| &t.kind),
                            Some(TokenKind::Colon)
                                | Some(TokenKind::Comma)
                                | Some(TokenKind::RParen)
                                | Some(TokenKind::Newline)
                        ),
                        _ => false,
                    };
                    if is_simple_name {
                        let (ns, ne) = self.expect_name()?;
                        Some(self.text_at(ns, ne).to_string())
                    } else {
                        let target_expr = self.parse_expr()?;
                        let span = Span::dummy();
                        let tmp = format!("__with_target_{}__", target_expr.span.start);
                        let tmp_ident = Spanned::new(Expr::Ident(tmp.clone()), span);
                        tuple_unpack_prepends.push(Spanned::new(
                            Stmt::Assign {
                                target: target_expr,
                                value: tmp_ident,
                            },
                            span,
                        ));
                        Some(tmp)
                    }
                }
            } else {
                None
            };
            items.push(WithItem { context, alias });
            if self.peek_kind() != Some(TokenKind::Comma) {
                break;
            }
            self.advance();
            // Allow trailing comma before `)`
            if parenthesized {
                self.skip_newlines();
                if self.peek_kind() == Some(TokenKind::RParen) {
                    break;
                }
            }
        }
        if parenthesized {
            self.skip_newlines();
            self.expect(TokenKind::RParen)?;
        }
        self.expect(TokenKind::Colon)?;
        let mut body = self.parse_block()?;
        // Prepend any tuple-unpack desugars (#1592) so `(a, b) = __with_target_N__`
        // runs at the start of the body and binds the user-visible names.
        if !tuple_unpack_prepends.is_empty() {
            let mut new_body = tuple_unpack_prepends;
            new_body.append(&mut body);
            body = new_body;
        }
        let stmt = if is_async {
            Stmt::AsyncWith { items, body }
        } else {
            Stmt::With { items, body }
        };
        Ok(Spanned::new(stmt, self.span_from(start)))
    }
}
