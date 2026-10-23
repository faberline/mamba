use super::*;

impl<'a> Parser<'a> {
    pub(super) fn parse_comprehension_target_atom(
        &mut self,
        stop: &TokenKind,
        targets: &mut Vec<String>,
        target_reads: &mut Vec<String>,
    ) -> crate::error::Result<bool> {
        let Some(kind) = self.peek_kind() else {
            return Err(crate::error::MambaError::syntax(
                Span::dummy(),
                "unexpected end of comprehension target",
            ));
        };

        match kind {
            TokenKind::Star => {
                self.advance();
                self.parse_comprehension_target_atom(stop, targets, target_reads)?;
                Ok(true)
            }
            TokenKind::LParen => {
                self.advance();
                let had_comma = self.parse_comprehension_target_list_until(
                    TokenKind::RParen,
                    targets,
                    target_reads,
                )?;
                self.expect(TokenKind::RParen)?;
                Ok(had_comma)
            }
            TokenKind::LBracket => {
                self.advance();
                self.parse_comprehension_target_list_until(
                    TokenKind::RBracket,
                    targets,
                    target_reads,
                )?;
                self.expect(TokenKind::RBracket)?;
                Ok(true)
            }
            ref name_kind if Self::is_name_token(name_kind) => {
                let (ts, te) = self.expect_name()?;
                let name = self.text_at(ts, te).to_string();
                let mut binds_name = true;
                loop {
                    match self.peek_kind() {
                        Some(TokenKind::Dot) => {
                            binds_name = false;
                            self.advance();
                            self.expect_name()?;
                        }
                        Some(TokenKind::LBracket) => {
                            binds_name = false;
                            self.consume_balanced_target_trailer(
                                TokenKind::LBracket,
                                TokenKind::RBracket,
                            )?;
                        }
                        Some(TokenKind::LParen) => {
                            binds_name = false;
                            self.consume_balanced_target_trailer(
                                TokenKind::LParen,
                                TokenKind::RParen,
                            )?;
                        }
                        _ => break,
                    }
                }
                if binds_name {
                    targets.push(name);
                } else {
                    target_reads.push(name);
                }
                Ok(false)
            }
            _ if self.peek_kind() == Some(stop.clone()) => Ok(false),
            _ => {
                let span = self
                    .peek()
                    .map(|token| Span::new(self.file_id, token.start, token.end))
                    .unwrap_or_else(Span::dummy);
                Err(crate::error::MambaError::syntax(
                    span,
                    format!("expected identifier, got {kind}"),
                ))
            }
        }
    }
}
