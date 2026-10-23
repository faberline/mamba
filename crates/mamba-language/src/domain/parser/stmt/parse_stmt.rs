use super::*;

/// Parse a single statement.
impl<'a> Parser<'a> {
    pub fn parse_stmt(&mut self) -> crate::error::Result<Spanned<Stmt>> {
        // If a previous call desugared something like `a = b = c = val` into
        // multiple per-target Assigns, hand those out first before consuming
        // any new tokens. Stored in reverse so pop() gives insertion order.
        if let Some(pending) = self.pending_stmts.pop() {
            return Ok(pending);
        }
        self.skip_newlines();
        let token = self
            .peek()
            .ok_or_else(|| MambaError::syntax(Span::dummy(), "unexpected end of input"))?;
        let start = token.start;

        match &token.kind {
            TokenKind::At => self.parse_decorated(),
            TokenKind::Def => self.parse_fn_def(vec![]),
            TokenKind::Async => self.parse_async_stmt(),
            TokenKind::Class => self.parse_class_def(vec![]),
            // `enum Foo:` (enum def) vs `enum = x` (expression/assignment)
            TokenKind::Enum => {
                if self.peek_at(1).is_some_and(Self::is_name_token) {
                    self.parse_enum_def()
                } else {
                    self.parse_ident_stmt()
                }
            }
            TokenKind::If => self.parse_if(),
            TokenKind::While => self.parse_while(),
            TokenKind::For => self.parse_for(false),
            // `match expr:` (match stmt) — `match` is a soft keyword in Python
            TokenKind::Match => {
                // match is a statement if there's a `:` at statement level after the subject.
                // Scan forward skipping balanced parens/brackets/braces to find a `:`.
                // If the token immediately after `match` is `=` or `.`, treat as ident.
                let next = self.peek_at(1);
                if next == Some(&TokenKind::Eq) || next == Some(&TokenKind::Dot) {
                    self.parse_ident_stmt()
                } else if self.match_subject_has_colon() {
                    self.parse_match()
                } else {
                    self.parse_ident_stmt()
                }
            }
            TokenKind::Return => self.parse_return(),
            TokenKind::Try => self.parse_try(),
            TokenKind::Raise => self.parse_raise(),
            TokenKind::With => self.parse_with(false),
            TokenKind::Assert => self.parse_assert(),
            TokenKind::Del => self.parse_del(),
            TokenKind::Global => self.parse_global(),
            TokenKind::Nonlocal => self.parse_nonlocal(),
            // `type X = int` (type alias) vs `type(x)` (expression)
            TokenKind::Type => {
                // Lookahead: type alias requires `type <Ident>`
                if self.peek_at(1).is_some_and(Self::is_name_token) {
                    self.parse_type_alias()
                } else {
                    self.parse_ident_stmt()
                }
            }
            TokenKind::Pass => {
                self.advance();
                self.skip_newlines();
                Ok(Spanned::new(Stmt::Pass, self.span_from(start)))
            }
            TokenKind::Break => {
                self.advance();
                self.skip_newlines();
                Ok(Spanned::new(Stmt::Break, self.span_from(start)))
            }
            TokenKind::Continue => {
                self.advance();
                self.skip_newlines();
                Ok(Spanned::new(Stmt::Continue, self.span_from(start)))
            }
            TokenKind::Import => self.parse_import(),
            TokenKind::From => self.parse_from_import(),
            // Brace-delimited expressions (dict/set literals) appearing as
            // the first token of a statement must route through the expression
            // parser via parse_ident_stmt(). Explicit arms prevent accidental
            // capture by future match arms and clarify that LBrace/LParen/
            // LBracket are valid statement-leading tokens (#1112).
            TokenKind::LBrace | TokenKind::LParen | TokenKind::LBracket => self.parse_ident_stmt(),
            // Any other expression-starting token: handles assignment, augassign,
            // tuple unpacking, var decl, and bare expression statements.
            _ => self.parse_ident_stmt(),
        }
    }
}
