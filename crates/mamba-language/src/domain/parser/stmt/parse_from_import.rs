use super::*;

impl<'a> Parser<'a> {
    pub(super) fn parse_from_import(&mut self) -> crate::error::Result<Spanned<Stmt>> {
        let (start, _) = self.advance();
        // Handle relative imports: `from . import x`, `from .foo import x`
        let mut module = Vec::new();
        let is_relative_start = matches!(
            self.peek_kind(),
            Some(TokenKind::Dot) | Some(TokenKind::Ellipsis)
        );
        if is_relative_start {
            // relative import — dots then optional module name.
            // `...` is lexed as Ellipsis (not 3 × Dot), so handle both.
            while matches!(
                self.peek_kind(),
                Some(TokenKind::Dot) | Some(TokenKind::Ellipsis)
            ) {
                match self.peek_kind() {
                    Some(TokenKind::Ellipsis) => {
                        self.advance();
                        module.push(".".to_string());
                        module.push(".".to_string());
                        module.push(".".to_string());
                    }
                    _ => {
                        self.advance();
                        module.push(".".to_string());
                    }
                }
            }
            if self.peek_kind().as_ref().is_some_and(Self::is_name_token) {
                let (s, e) = self.expect_name()?;
                module.push(self.text_at(s, e).to_string());
                while self.peek_kind() == Some(TokenKind::Dot) {
                    self.advance();
                    let (s, e) = self.expect_name()?;
                    module.push(self.text_at(s, e).to_string());
                }
            }
        } else {
            let (s, e) = self.expect_name()?;
            module.push(self.text_at(s, e).to_string());
            while self.peek_kind() == Some(TokenKind::Dot) {
                self.advance();
                let (s, e) = self.expect_name()?;
                module.push(self.text_at(s, e).to_string());
            }
        }
        self.expect(TokenKind::Import)?;
        // Handle `from x import *`
        if self.peek_kind() == Some(TokenKind::Star) {
            self.advance();
            self.skip_newlines();
            return Ok(Spanned::new(
                Stmt::Import {
                    module,
                    names: Some(vec![("*".to_string(), None)]),
                    module_alias: None,
                },
                self.span_from(start),
            ));
        }
        // Handle `from x import (a, b, c)`
        let paren = self.peek_kind() == Some(TokenKind::LParen);
        if paren {
            self.advance();
        }
        let mut names = Vec::new();
        loop {
            if paren && self.peek_kind() == Some(TokenKind::RParen) {
                break;
            }
            self.skip_newlines();
            let (s, e) = self.expect_name()?;
            let name = self.text_at(s, e).to_string();
            let alias = if self.peek_kind() == Some(TokenKind::As) {
                self.advance();
                let (s, e) = self.expect_name()?;
                Some(self.text_at(s, e).to_string())
            } else {
                None
            };
            names.push((name, alias));
            if self.peek_kind() != Some(TokenKind::Comma) {
                break;
            }
            self.advance();
            self.skip_newlines();
        }
        if paren {
            self.expect(TokenKind::RParen)?;
        }
        self.skip_newlines();
        Ok(Spanned::new(
            Stmt::Import {
                module,
                names: Some(names),
                module_alias: None,
            },
            self.span_from(start),
        ))
    }
}
