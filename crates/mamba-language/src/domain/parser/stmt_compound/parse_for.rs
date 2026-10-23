use super::*;

/// Compound statement parsers: def, class, enum, if, while, for, match,
/// try, raise, with, assert, del, global, nonlocal, type alias, decorated.
impl<'a> Parser<'a> {
    pub(crate) fn parse_for(&mut self, is_async: bool) -> crate::error::Result<Spanned<Stmt>> {
        let (start, _) = self.advance(); // consume `for`
                                         // Parse comma-separated target units: each unit is either a bare
                                         // name or a parenthesized group `(a, b, ...)`.
                                         //   #1590: outer parens around the *whole* target list (single
                                         //          group) flatten directly into `targets`.
                                         //   #1594: per-unit parenthesized groups in non-leading position
                                         //          (e.g. `for (a, b), c in ...` or `for (a, b), (c, d) in ...`)
                                         //          desugar into a fresh `__for_target_N_K__` flat target
                                         //          plus a prepended `(orig...) = __for_target_N_K__` in the
                                         //          body.
        let mut targets: Vec<String> = Vec::new();
        let mut prepends: Vec<Spanned<Stmt>> = Vec::new();
        let mut units: Vec<(Option<Vec<String>>, String, bool)> = Vec::new();
        loop {
            // A per-unit target group may be written with either delimiter:
            // `(a, b)` or the equivalent list-display `[a, b]` (#1927) — both
            // desugar identically via the `__for_target_N_K__` flattening
            // below, so only the open/close token kinds differ.
            let group_close = if self.peek_kind() == Some(TokenKind::LParen) {
                Some(TokenKind::RParen)
            } else if self.peek_kind() == Some(TokenKind::LBracket) {
                Some(TokenKind::RBracket)
            } else {
                None
            };
            if let Some(close_kind) = group_close {
                let (lp_start, _) = self.advance();
                let mut group: Vec<String> = Vec::new();
                let (s, e) = self.expect_name()?;
                group.push(self.text_at(s, e).to_string());
                while self.peek_kind() == Some(TokenKind::Comma) {
                    self.advance();
                    if self.peek_kind() == Some(close_kind.clone()) {
                        break;
                    }
                    let (s, e) = self.expect_name()?;
                    group.push(self.text_at(s, e).to_string());
                }
                self.expect(close_kind)?;
                units.push((
                    Some(group),
                    format!("__for_target_{}_{}__", lp_start, units.len()),
                    false,
                ));
            } else {
                // PEP 3132: a starred name (`for a, *b in seq:`) collects the
                // remaining iteration-value elements into a list (#1895).
                let starred = if self.peek_kind() == Some(TokenKind::Star) {
                    self.advance();
                    true
                } else {
                    false
                };
                let (s, e) = self.expect_name()?;
                units.push((None, self.text_at(s, e).to_string(), starred));
            }
            if self.peek_kind() == Some(TokenKind::Comma) {
                self.advance();
                if self.peek_kind() == Some(TokenKind::In) {
                    break;
                }
            } else {
                break;
            }
        }
        if units.iter().filter(|(_, _, starred)| *starred).count() > 1 {
            return Err(MambaError::syntax(
                self.span_from(start),
                "multiple starred expressions in assignment".to_string(),
            ));
        }
        let mut star_index = None;
        // Single outer-paren group with no continuation → treat as flat.
        if units.len() == 1 {
            match units.into_iter().next().unwrap() {
                (Some(group), _, _) => {
                    targets = group;
                }
                (None, _, true) => {
                    return Err(MambaError::syntax(
                        self.span_from(start),
                        "starred assignment target must be in a list or tuple".to_string(),
                    ));
                }
                (None, name, false) => {
                    targets.push(name);
                }
            }
        } else {
            for (idx, (group, flat_name, starred)) in units.into_iter().enumerate() {
                if starred {
                    star_index = Some(idx);
                }
                targets.push(flat_name.clone());
                if let Some(names) = group {
                    let span = Span::dummy();
                    let tuple_elems: Vec<Spanned<Expr>> = names
                        .into_iter()
                        .map(|n| Spanned::new(Expr::Ident(n), span))
                        .collect();
                    let tuple_target = Spanned::new(Expr::TupleLit(tuple_elems), span);
                    let tmp_ident = Spanned::new(Expr::Ident(flat_name), span);
                    prepends.push(Spanned::new(
                        Stmt::Assign {
                            target: tuple_target,
                            value: tmp_ident,
                        },
                        span,
                    ));
                }
            }
        }
        // Optional type annotation (single target only)
        let var_ty = if targets.len() == 1 && self.peek_kind() == Some(TokenKind::Colon) {
            self.advance();
            Some(self.parse_type_expr()?)
        } else {
            None
        };
        self.expect(TokenKind::In)?;
        let iter = self.parse_tuple_or_expr()?;
        self.expect(TokenKind::Colon)?;
        let mut body = self.parse_block()?;
        if !prepends.is_empty() {
            let mut new_body = prepends;
            new_body.append(&mut body);
            body = new_body;
        }
        let else_body = if self.peek_kind() == Some(TokenKind::Else) {
            self.advance();
            self.expect(TokenKind::Colon)?;
            Some(self.parse_block()?)
        } else {
            None
        };
        let stmt = if is_async {
            Stmt::AsyncFor {
                targets,
                star_index,
                var_ty,
                iter,
                body,
                else_body,
            }
        } else {
            Stmt::For {
                targets,
                star_index,
                var_ty,
                iter,
                body,
                else_body,
            }
        };
        Ok(Spanned::new(stmt, self.span_from(start)))
    }
}
