pub mod ast;
pub mod expr;
pub mod expr_compound;
pub(crate) mod mangle;
pub mod pattern;
pub mod stmt;
pub mod stmt_compound;
pub mod type_expr;
mod unicode;

use crate::error::MambaError;
use crate::source::span::{FileId, Span, Spanned};
use ast::{Module, Stmt};
use mamba_shared_kernel::token::{Token, TokenKind};

pub type RawLexer = fn(&str, FileId) -> Vec<Token>;

/// Recursive descent parser for Mamba source code.
pub struct Parser<'a> {
    raw_lexer: RawLexer,
    tokens: Vec<Token>,
    pos: usize,
    source: &'a str,
    file_id: FileId,
    /// Statements that parse_stmt synthesized but hasn't yet returned.
    /// Populated only for forms like `a = b = c = expr` that desugar into
    /// multiple per-target `Stmt::Assign`s. Drained in LIFO order before
    /// the parser consumes any new tokens, so insertion order is preserved.
    pub(crate) pending_stmts: Vec<Spanned<Stmt>>,
    /// True when the next `parse_expr` is the top of an expression statement,
    /// where a bare walrus (`a := 5`) is a SyntaxError in CPython (it must be
    /// parenthesized). Captured-and-cleared at `parse_expr` entry so only the
    /// statement-top level — not parenthesized or nested sub-expressions — is
    /// affected.
    pub(crate) stmt_expr_toplevel: bool,
    /// True while parsing statements that execute directly in a class body.
    /// Function and lambda bodies nested inside a class temporarily clear this.
    pub(crate) in_class_body: bool,
}

impl<'a> Parser<'a> {
    pub fn new(tokens: Vec<Token>, source: &'a str, file_id: FileId, raw_lexer: RawLexer) -> Self {
        let tokens = if unicode::tokens_need_unicode_ident_repair(&tokens, source) {
            unicode::prepare_parser_tokens(source, file_id, raw_lexer)
        } else {
            tokens
        };
        Self {
            raw_lexer,
            tokens,
            pos: 0,
            source,
            file_id,
            pending_stmts: Vec::new(),
            stmt_expr_toplevel: false,
            in_class_body: false,
        }
    }

    /// Parse the entire module.
    pub fn parse_module(&mut self) -> crate::error::Result<Module> {
        let mut stmts = Vec::new();
        self.skip_newlines();
        while !self.pending_stmts.is_empty()
            || (self.peek_kind() != Some(TokenKind::Eof) && self.peek_kind().is_some())
        {
            if self.pending_stmts.is_empty() {
                self.skip_newlines();
                if self.peek_kind() == Some(TokenKind::Eof) || self.peek_kind().is_none() {
                    break;
                }
            }
            stmts.push(self.parse_stmt()?);
            // Semicolons as statement separators between simple statements.
            while self.peek_kind() == Some(TokenKind::Semicolon) {
                self.advance(); // consume `;`
                                // Skip consecutive semicolons (empty statements)
                while self.peek_kind() == Some(TokenKind::Semicolon) {
                    self.advance();
                }
                // Trailing semicolon before newline/eof — stop
                if self.peek_kind() == Some(TokenKind::Newline)
                    || (self.pending_stmts.is_empty()
                        && (self.peek_kind() == Some(TokenKind::Eof) || self.peek_kind().is_none()))
                {
                    break;
                }
                // Compound statements are not allowed after `;`
                if let Some(kind) = self.peek_kind() {
                    if Self::is_compound_start(&kind) {
                        return Err(MambaError::syntax(
                            self.peek()
                                .map(|t| Span::new(self.file_id, t.start, t.end))
                                .unwrap_or(Span::dummy()),
                            "compound statement not allowed after semicolon".to_string(),
                        ));
                    }
                }
                stmts.push(self.parse_stmt()?);
            }
            self.skip_newlines();
        }
        Ok(Module { stmts })
    }

    // --- Token navigation ---

    pub fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    pub fn peek_kind(&self) -> Option<TokenKind> {
        self.peek().map(|t| t.kind.clone())
    }

    /// Advance past the current token, returning its (start, end) byte offsets.
    pub(crate) fn advance(&mut self) -> (u32, u32) {
        let token = &self.tokens[self.pos];
        let result = (token.start, token.end);
        self.pos += 1;
        result
    }

    /// Expect a specific token kind, returning its (start, end) byte offsets.
    pub(crate) fn expect(&mut self, expected: TokenKind) -> crate::error::Result<(u32, u32)> {
        let pos = self.pos;
        let token = self.tokens.get(pos).ok_or_else(|| {
            MambaError::syntax(Span::dummy(), format!("expected {expected}, got EOF"))
        })?;
        if std::mem::discriminant(&token.kind) != std::mem::discriminant(&expected) {
            return Err(MambaError::syntax(
                Span::new(self.file_id, token.start, token.end),
                format!("expected {expected}, got {}", token.kind),
            ));
        }
        self.pos += 1;
        Ok((self.tokens[pos].start, self.tokens[pos].end))
    }

    /// Get the text content at a given byte range from the source.
    pub(crate) fn text_at(&self, start: u32, end: u32) -> &str {
        &self.source[start as usize..end as usize]
    }

    pub(crate) fn current_text(&self) -> &str {
        let token = &self.tokens[self.pos];
        token.text(self.source)
    }

    pub fn skip_newlines(&mut self) {
        while self.peek_kind() == Some(TokenKind::Newline) {
            self.advance();
        }
    }

    pub(crate) fn span_from(&self, start: u32) -> Span {
        let end = if self.pos > 0 {
            self.tokens[self.pos - 1].end
        } else {
            start
        };
        Span::new(self.file_id, start, end)
    }

    /// Check if a token starts a compound statement (if/while/for/def/class/etc.).
    /// These are not allowed after a semicolon separator.
    pub(crate) fn is_compound_start(kind: &TokenKind) -> bool {
        matches!(
            kind,
            TokenKind::If
                | TokenKind::While
                | TokenKind::For
                | TokenKind::Def
                | TokenKind::Class
                | TokenKind::Async
                | TokenKind::Try
                | TokenKind::With
                | TokenKind::At
        )
    }

    /// Check if the current token can be used as a name (identifier or soft keyword).
    /// Python allows `int`, `float`, `bool`, `str`, `list`, `dict`, `tuple`, `type`
    /// as variable/parameter/attribute names.
    pub(crate) fn is_name_token(kind: &TokenKind) -> bool {
        matches!(
            kind,
            TokenKind::Ident
                | TokenKind::Self_
                | TokenKind::IntType
                | TokenKind::FloatType
                | TokenKind::BoolType
                | TokenKind::StrType
                | TokenKind::ListType
                | TokenKind::DictType
                | TokenKind::TupleType
                | TokenKind::Type
                | TokenKind::Match
                | TokenKind::Case
                | TokenKind::Enum
        )
    }

    /// Lookahead helper used by lambda type-annotation disambiguation
    /// (#1582). Used to distinguish a typed-lambda continuation
    /// (`lambda x: T, y: U: body` — a further typed param follows) from a
    /// lambda whose body is part of an enclosing container
    /// (`lambda x: x, rest_of_args` in a call/list/dict/tuple — no further
    /// param follows; the comma belongs to the enclosing container).
    ///
    /// A flat scan for "any depth-0 colon ahead" is not sufficient here
    /// (#1918): an enclosing dict literal's own sibling entries
    /// (`{1: lambda x: x, 2: 'next'}`) sit at the same token depth as a
    /// further lambda parameter would, since the dict's own opening `{`
    /// was consumed before this scan's start and so never registers as a
    /// depth-changing token relative to it. The dict's next `2: 'next'`
    /// entry then looks identical, depth-wise, to a genuine continuing
    /// param. So instead of hunting for any colon, speculatively parse the
    /// comma-separated tail as if it really were more
    /// `name (: type)? (= default)?` parameters, and only confirm when
    /// that tail is well-formed and lands on the real body-separator `:`.
    /// Restores `self.pos` unconditionally — this is a pure lookahead.
    pub(crate) fn has_lambda_body_colon_ahead(&mut self) -> bool {
        let saved = self.pos;
        let confirmed = self.scan_further_typed_lambda_params();
        self.pos = saved;
        confirmed
    }

    /// Consumes zero-or-more `, name (: type)? (= default)?` groups from
    /// the current `,`, returning true iff that chain is immediately
    /// followed by the lambda's body-separator `:`. Any shape that isn't
    /// a parameter (e.g. a non-name token right after the comma, as in a
    /// dict's next key, or a call expression like `R(...)`) returns false
    /// so the caller backtracks to the body-separator interpretation.
    fn scan_further_typed_lambda_params(&mut self) -> bool {
        loop {
            if self.peek_kind() != Some(TokenKind::Comma) {
                return false;
            }
            self.advance(); // consume ,
            if matches!(
                self.peek_kind(),
                Some(TokenKind::Star) | Some(TokenKind::DoubleStar)
            ) {
                self.advance();
            }
            if !self.peek_kind().as_ref().is_some_and(Self::is_name_token) {
                return false;
            }
            self.advance(); // consume the name
            if self.peek_kind() == Some(TokenKind::Colon) {
                self.advance();
                if self.parse_type_expr().is_err() {
                    return false;
                }
            }
            if self.peek_kind() == Some(TokenKind::Eq) {
                self.advance();
                if self.parse_expr().is_err() {
                    return false;
                }
            }
            match self.peek_kind() {
                Some(TokenKind::Colon) => return true,
                Some(TokenKind::Comma) => continue,
                _ => return false,
            }
        }
    }

    /// Expect a name token (identifier or soft keyword), returning (start, end).
    pub(crate) fn expect_name(&mut self) -> crate::error::Result<(u32, u32)> {
        let token = self.tokens.get(self.pos).ok_or_else(|| {
            MambaError::syntax(Span::dummy(), "expected identifier, got EOF".to_string())
        })?;
        if Self::is_name_token(&token.kind) {
            let result = (token.start, token.end);
            self.pos += 1;
            Ok(result)
        } else {
            Err(MambaError::syntax(
                Span::new(self.file_id, token.start, token.end),
                format!("expected identifier, got {}", token.kind),
            ))
        }
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod inline_integration_tests;
