use super::ast::TypeExpr;
use super::Parser;
use crate::error::MambaError;
use crate::source::span::{Span, Spanned};
use mamba_shared_kernel::token::TokenKind;

impl<'a> Parser<'a> {
    /// Parse a type annotation expression.
    pub fn parse_type_expr(&mut self) -> crate::error::Result<Spanned<TypeExpr>> {
        let mut ty = self.parse_type_atom()?;

        // Check for union: `T | U`
        if self.peek_kind() == Some(TokenKind::Pipe) {
            let mut types = vec![ty];
            while self.peek_kind() == Some(TokenKind::Pipe) {
                self.advance();
                types.push(self.parse_type_atom()?);
            }
            let span = types
                .first()
                .unwrap()
                .span
                .merge(types.last().unwrap().span);
            ty = Spanned::new(TypeExpr::Union(types), span);
        }

        // Check for optional shorthand: `T?`
        if self.peek_kind() == Some(TokenKind::Question) {
            self.advance();
            let span = ty.span.merge(self.span_from(ty.span.start));
            ty = Spanned::new(TypeExpr::Optional(Box::new(ty)), span);
        }

        Ok(ty)
    }

    fn parse_type_atom(&mut self) -> crate::error::Result<Spanned<TypeExpr>> {
        let token = self
            .peek()
            .ok_or_else(|| MambaError::syntax(Span::dummy(), "expected type expression"))?;
        let start = token.start;

        if token.kind == TokenKind::Star {
            self.advance();
            let inner = self.parse_type_atom()?;
            return Ok(Spanned::new(
                TypeExpr::Unpack(Box::new(inner)),
                self.span_from(start),
            ));
        }

        match &token.kind {
            // Built-in type keywords
            TokenKind::IntType
            | TokenKind::FloatType
            | TokenKind::BoolType
            | TokenKind::StrType => {
                let name = self.current_text().to_string();
                self.advance();
                Ok(Spanned::new(TypeExpr::Named(name), self.span_from(start)))
            }
            // Generic container types: list[T], dict[K, V], tuple[T, U]
            TokenKind::ListType | TokenKind::DictType | TokenKind::TupleType => {
                let name = self.current_text().to_string();
                self.advance();
                if self.peek_kind() == Some(TokenKind::LBracket) {
                    self.advance();
                    let args = self.parse_type_arg_list()?;
                    self.expect(TokenKind::RBracket)?;
                    let span = self.span_from(start);
                    if name == "tuple" {
                        Ok(Spanned::new(TypeExpr::Tuple(args), span))
                    } else {
                        Ok(Spanned::new(TypeExpr::Generic { name, args }, span))
                    }
                } else {
                    Ok(Spanned::new(TypeExpr::Named(name), self.span_from(start)))
                }
            }
            // User-defined type or generic: `MyType`, `MyType[T]`,
            // or dotted reference: `collections.abc.Mapping` (#1576).
            TokenKind::Ident => {
                let mut name = self.current_text().to_string();
                self.advance();
                while self.peek_kind() == Some(TokenKind::Dot) {
                    self.advance();
                    let (s, e) = self.expect_name()?;
                    name.push('.');
                    name.push_str(self.text_at(s, e));
                }
                if self.peek_kind() == Some(TokenKind::LParen) {
                    self.parse_type_call_annotation(start, &name)
                } else if self.peek_kind() == Some(TokenKind::LBracket) {
                    self.advance();
                    let args = self.parse_type_arg_list()?;
                    self.expect(TokenKind::RBracket)?;
                    Ok(Spanned::new(
                        TypeExpr::Generic { name, args },
                        self.span_from(start),
                    ))
                } else {
                    Ok(Spanned::new(TypeExpr::Named(name), self.span_from(start)))
                }
            }
            // Parenthesized type or function type
            TokenKind::LParen => {
                self.advance();
                let mut params = Vec::new();
                while self.peek_kind() != Some(TokenKind::RParen)
                    && self.peek_kind() != Some(TokenKind::Eof)
                {
                    params.push(self.parse_type_expr()?);
                    if self.peek_kind() != Some(TokenKind::RParen) {
                        self.expect(TokenKind::Comma)?;
                    }
                }
                self.expect(TokenKind::RParen)?;

                // Check for arrow -> to make it a function type
                if self.peek_kind() == Some(TokenKind::Arrow) {
                    self.advance();
                    let ret = self.parse_type_expr()?;
                    let span = self.span_from(start);
                    Ok(Spanned::new(
                        TypeExpr::Fn {
                            params,
                            ret: Box::new(ret),
                        },
                        span,
                    ))
                } else if params.len() == 1 {
                    Ok(params.into_iter().next().unwrap())
                } else {
                    Ok(Spanned::new(TypeExpr::Tuple(params), self.span_from(start)))
                }
            }
            // String literal type annotation: `-> "TypeName"` (PEP 484 forward reference).
            // Preserve that it came from a string so type checking treats it as
            // a forward reference while runtime introspection sees the text.
            TokenKind::Str(v) | TokenKind::TripleStr(v) | TokenKind::RawStr(v) => {
                let name = v.clone();
                self.advance();
                Ok(Spanned::new(
                    TypeExpr::Named(super::ast::forward_ref_name(&name)),
                    self.span_from(start),
                ))
            }
            TokenKind::None_ => {
                self.advance();
                Ok(Spanned::new(
                    TypeExpr::Named("None".to_string()),
                    self.span_from(start),
                ))
            }
            // `type` used as a type expression (e.g. `type[BaseModel]`).
            // Python's `type[X]` is the builtin generic for the class-object of X.
            // `type` is a soft keyword — in type annotation position it acts as an identifier.
            TokenKind::Type => {
                self.advance(); // consume 'type'
                if self.peek_kind() == Some(TokenKind::LBracket) {
                    self.advance(); // consume '['
                    let args = self.parse_type_arg_list()?;
                    self.expect(TokenKind::RBracket)?;
                    Ok(Spanned::new(
                        TypeExpr::Generic {
                            name: "type".to_string(),
                            args,
                        },
                        self.span_from(start),
                    ))
                } else if self.peek_kind() == Some(TokenKind::LParen) {
                    // `type` is a lexer soft-keyword token (TokenKind::Type),
                    // distinct from TokenKind::Ident, so it doesn't fall into
                    // the Ident branch's existing call-annotation handling
                    // above (#1929) — a call expression here (`type(None)`,
                    // e.g. `Union[int, type(None)]`) is annotation-position
                    // PEP 3107 syntax, never validated as a real type.
                    self.parse_type_call_annotation(start, "type")
                } else {
                    Ok(Spanned::new(
                        TypeExpr::Named("type".to_string()),
                        self.span_from(start),
                    ))
                }
            }
            // Arbitrary non-type annotation expression: `-> 1`, `x: {1: 2}`,
            // `-> [int]`, `-> a + b` (PEP 3107). Python annotations are arbitrary
            // expressions and are never validated as types — CPython only stores them
            // in __annotations__ — so a non-type expression carries no constraint and
            // resolves to `Any`. Consume the whole expression to stay token-synced,
            // rather than failing the parse. Genuinely invalid syntax still errors via
            // parse_expr below.
            //
            // Simple numeric/bool literals keep their textual form as the Named
            // type so introspection (`str(inspect.signature(f))` rendering
            // `-> 42`) sees the declared annotation; the checker resolves any
            // unknown Named type to Any, so typing is unaffected.
            _ => {
                let expr = self.parse_expr()?;
                let lit_name = match &expr.node {
                    super::ast::Expr::IntLit(v) => Some(v.to_string()),
                    super::ast::Expr::BigIntLit(v) => Some(v.clone()),
                    super::ast::Expr::FloatLit(v) => Some(v.to_string()),
                    super::ast::Expr::BoolLit(b) => Some(if *b {
                        "True".to_string()
                    } else {
                        "False".to_string()
                    }),
                    _ => None,
                };
                Ok(Spanned::new(
                    TypeExpr::Named(lit_name.unwrap_or_else(|| "Any".to_string())),
                    self.span_from(start),
                ))
            }
        }
    }

    fn parse_type_arg_list(&mut self) -> crate::error::Result<Vec<Spanned<TypeExpr>>> {
        let mut args = vec![self.parse_type_expr()?];
        while self.peek_kind() == Some(TokenKind::Comma) {
            self.advance();
            if self.peek_kind() == Some(TokenKind::RBracket) {
                break;
            }
            args.push(self.parse_type_expr()?);
        }
        Ok(args)
    }

    fn parse_type_call_annotation(
        &mut self,
        start: u32,
        name: &str,
    ) -> crate::error::Result<Spanned<TypeExpr>> {
        self.expect(TokenKind::LParen)?;
        let mut forward_arg = None;
        if self.peek_kind() != Some(TokenKind::RParen) {
            let first_arg = self.parse_expr()?;
            if name.rsplit('.').next() == Some("ForwardRef") {
                if let super::ast::Expr::StrLit(s) = first_arg.node {
                    forward_arg = Some(s);
                }
            }
            let mut depth = 0usize;
            while let Some(kind) = self.peek_kind() {
                match kind {
                    TokenKind::RParen if depth == 0 => break,
                    TokenKind::LParen | TokenKind::LBracket | TokenKind::LBrace => {
                        depth += 1;
                        self.advance();
                    }
                    TokenKind::RParen | TokenKind::RBracket | TokenKind::RBrace => {
                        depth = depth.saturating_sub(1);
                        self.advance();
                    }
                    TokenKind::Eof => {
                        return Err(MambaError::syntax(
                            self.span_from(start),
                            "unterminated annotation call",
                        ));
                    }
                    _ => {
                        self.advance();
                    }
                }
            }
        }
        self.expect(TokenKind::RParen)?;
        let name = forward_arg
            .map(|s| super::ast::forward_ref_name(&s))
            .unwrap_or_else(|| "Any".to_string());
        Ok(Spanned::new(TypeExpr::Named(name), self.span_from(start)))
    }
}

#[cfg(test)]
mod tests;
