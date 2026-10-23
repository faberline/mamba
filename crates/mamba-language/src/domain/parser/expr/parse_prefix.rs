use super::*;

impl<'a> Parser<'a> {
    pub(super) fn parse_prefix(&mut self) -> crate::error::Result<Spanned<Expr>> {
        let token = self
            .peek()
            .ok_or_else(|| MambaError::syntax(Span::dummy(), "unexpected end of input"))?;
        let start = token.start;

        match &token.kind {
            TokenKind::Int(v) => {
                let v = *v;
                let (_tok_start, tok_end) = self.advance();
                let span = self.span_from(start);
                if v == BIG_INT_LITERAL_SENTINEL {
                    Ok(Spanned::new(
                        Expr::BigIntLit(self.text_at(start, tok_end).to_string()),
                        span,
                    ))
                } else {
                    Ok(Spanned::new(Expr::IntLit(v), span))
                }
            }
            TokenKind::Float(v) => {
                let v = *v;
                self.advance();
                Ok(Spanned::new(Expr::FloatLit(v), self.span_from(start)))
            }
            TokenKind::Complex(v) => {
                let v = *v;
                self.advance();
                Ok(Spanned::new(Expr::ComplexLit(v), self.span_from(start)))
            }
            TokenKind::Str(v) | TokenKind::TripleStr(v) | TokenKind::RawStr(v) => {
                let mut buf = v.clone();
                self.advance();
                // PEP 3105 implicit string-literal concatenation.
                // Adjacent Str/TripleStr/RawStr tokens — Logos already
                // skipped whitespace and the IndentProcessor suppresses
                // newlines inside parens — concatenate at compile time.
                loop {
                    match self.peek_kind() {
                        Some(TokenKind::Str(s))
                        | Some(TokenKind::TripleStr(s))
                        | Some(TokenKind::RawStr(s)) => {
                            buf.push_str(&s);
                            self.advance();
                        }
                        Some(TokenKind::FStr(s)) | Some(TokenKind::RawFStr(s)) => {
                            // str + f-string promotes the whole expression
                            // to an FString whose first part is the buffered
                            // literal, followed by the f-string's parts.
                            let is_raw = matches!(self.peek_kind(), Some(TokenKind::RawFStr(_)));
                            self.advance();
                            let mut parts = vec![FStringPart::Literal(buf)];
                            parts.extend(
                                parse_fstring_parts_with_lexer(&s, is_raw, self.raw_lexer)
                                    .map_err(|m| {
                                        crate::error::MambaError::syntax(self.span_from(start), m)
                                    })?,
                            );
                            // keep absorbing trailing literals/f-strings.
                            loop {
                                match self.peek_kind() {
                                    Some(TokenKind::Str(s2))
                                    | Some(TokenKind::TripleStr(s2))
                                    | Some(TokenKind::RawStr(s2)) => {
                                        parts.push(FStringPart::Literal(s2));
                                        self.advance();
                                    }
                                    Some(TokenKind::FStr(s2)) | Some(TokenKind::RawFStr(s2)) => {
                                        let raw2 =
                                            matches!(self.peek_kind(), Some(TokenKind::RawFStr(_)));
                                        self.advance();
                                        parts.extend(
                                            parse_fstring_parts_with_lexer(
                                                &s2,
                                                raw2,
                                                self.raw_lexer,
                                            )
                                            .map_err(
                                                |m| {
                                                    crate::error::MambaError::syntax(
                                                        self.span_from(start),
                                                        m,
                                                    )
                                                },
                                            )?,
                                        );
                                    }
                                    _ => break,
                                }
                            }
                            return Ok(Spanned::new(Expr::FString(parts), self.span_from(start)));
                        }
                        _ => break,
                    }
                }
                Ok(Spanned::new(Expr::StrLit(buf), self.span_from(start)))
            }
            TokenKind::ByteStr(v) => {
                let v = v.clone();
                self.advance();
                let mut acc = crate::domain::literal::apply_bytes_escapes(&v);
                // Bytes literals concatenate the same way; mixing bytes and
                // str is a Python TypeError, so we stop at non-ByteStr.
                while let Some(TokenKind::ByteStr(s)) = self.peek_kind() {
                    self.advance();
                    let next = crate::domain::literal::apply_bytes_escapes(&s);
                    acc.extend_from_slice(&next);
                }
                Ok(Spanned::new(Expr::BytesLit(acc), self.span_from(start)))
            }
            TokenKind::FStr(v) | TokenKind::RawFStr(v) => {
                let v = v.clone();
                let is_raw = matches!(self.peek_kind(), Some(TokenKind::RawFStr(_)));
                self.advance();
                let mut parts = parse_fstring_parts_with_lexer(&v, is_raw, self.raw_lexer)
                    .map_err(|m| crate::error::MambaError::syntax(self.span_from(start), m))?;
                // f-string + (str | f-string) implicit join.
                loop {
                    match self.peek_kind() {
                        Some(TokenKind::Str(s))
                        | Some(TokenKind::TripleStr(s))
                        | Some(TokenKind::RawStr(s)) => {
                            parts.push(FStringPart::Literal(s));
                            self.advance();
                        }
                        Some(TokenKind::FStr(s)) | Some(TokenKind::RawFStr(s)) => {
                            let raw2 = matches!(self.peek_kind(), Some(TokenKind::RawFStr(_)));
                            self.advance();
                            parts.extend(
                                parse_fstring_parts_with_lexer(&s, raw2, self.raw_lexer).map_err(
                                    |m| crate::error::MambaError::syntax(self.span_from(start), m),
                                )?,
                            );
                        }
                        _ => break,
                    }
                }
                Ok(Spanned::new(Expr::FString(parts), self.span_from(start)))
            }
            TokenKind::True => {
                self.advance();
                Ok(Spanned::new(Expr::BoolLit(true), self.span_from(start)))
            }
            TokenKind::False => {
                self.advance();
                Ok(Spanned::new(Expr::BoolLit(false), self.span_from(start)))
            }
            TokenKind::None_ => {
                self.advance();
                Ok(Spanned::new(Expr::NoneLit, self.span_from(start)))
            }
            TokenKind::Ellipsis => {
                self.advance();
                Ok(Spanned::new(Expr::Ellipsis, self.span_from(start)))
            }
            TokenKind::Ident => {
                let name = self.current_text().to_string();
                self.advance();
                Ok(Spanned::new(Expr::Ident(name), self.span_from(start)))
            }
            TokenKind::Self_ => {
                self.advance();
                Ok(Spanned::new(
                    Expr::Ident("self".to_string()),
                    self.span_from(start),
                ))
            }
            TokenKind::Plus => {
                self.advance();
                let bp = prefix_bp(&UnaryOp::Pos);
                let operand = self.parse_expr_bp(bp)?;
                let span = Span::new(self.file_id, start, operand.span.end);
                Ok(Spanned::new(
                    Expr::UnaryOp {
                        op: UnaryOp::Pos,
                        operand: Box::new(operand),
                    },
                    span,
                ))
            }
            TokenKind::Minus => {
                self.advance();
                let bp = prefix_bp(&UnaryOp::Neg);
                let operand = self.parse_expr_bp(bp)?;
                let span = Span::new(self.file_id, start, operand.span.end);
                Ok(Spanned::new(
                    Expr::UnaryOp {
                        op: UnaryOp::Neg,
                        operand: Box::new(operand),
                    },
                    span,
                ))
            }
            TokenKind::Not => {
                self.advance();
                let bp = prefix_bp(&UnaryOp::Not);
                let operand = self.parse_expr_bp(bp)?;
                let span = Span::new(self.file_id, start, operand.span.end);
                Ok(Spanned::new(
                    Expr::UnaryOp {
                        op: UnaryOp::Not,
                        operand: Box::new(operand),
                    },
                    span,
                ))
            }
            TokenKind::Tilde => {
                self.advance();
                let bp = prefix_bp(&UnaryOp::BitNot);
                let operand = self.parse_expr_bp(bp)?;
                let span = Span::new(self.file_id, start, operand.span.end);
                Ok(Spanned::new(
                    Expr::UnaryOp {
                        op: UnaryOp::BitNot,
                        operand: Box::new(operand),
                    },
                    span,
                ))
            }
            // `await` in operand position (e.g. `-await f()`, `await f() * 10`).
            // It binds at the unary level — tighter than binary operators — so
            // those parse as `-(await f())` and `(await f()) * 10`. (Top-level
            // `await …` at the start of an expression flows through the same
            // prefix parser so `await a() + await b()` becomes
            // `(await a()) + (await b())`, matching Python precedence.
            TokenKind::Await => self.parse_await_expr(),
            // Type keywords usable as expressions: int(x), bool(x), etc.
            TokenKind::IntType
            | TokenKind::FloatType
            | TokenKind::BoolType
            | TokenKind::StrType
            | TokenKind::ListType
            | TokenKind::DictType
            | TokenKind::TupleType
            | TokenKind::Type
            | TokenKind::Enum
            | TokenKind::Match
            | TokenKind::Case => {
                let name = self.current_text().to_string();
                self.advance();
                Ok(Spanned::new(Expr::Ident(name), self.span_from(start)))
            }
            TokenKind::LParen => self.parse_paren_expr(),
            TokenKind::LBracket => self.parse_list_or_comp(),
            TokenKind::LBrace => self.parse_brace_expr(),
            _ => Err(MambaError::syntax(
                Span::new(self.file_id, start, token.end),
                format!("unexpected token: {}", token.kind),
            )),
        }
    }
}
