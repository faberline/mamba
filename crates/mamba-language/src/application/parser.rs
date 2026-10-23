use mamba_shared_kernel::ast::{Expr, Module, Pattern, Stmt, TypeExpr};
use mamba_shared_kernel::source::span::{FileId, Spanned};
use mamba_shared_kernel::token::{Token, TokenKind};

/// Public parsing service. It owns the grammar state without copying tokens.
pub struct Parser<'a>(crate::domain::parser::Parser<'a>);

impl<'a> Parser<'a> {
    pub fn new(tokens: Vec<Token>, source: &'a str, file_id: FileId) -> Self {
        Self(crate::domain::parser::Parser::new(
            tokens,
            source,
            file_id,
            crate::app::RAW_LEXER,
        ))
    }
    pub fn peek(&self) -> Option<&Token> {
        self.0.peek()
    }
    pub fn peek_kind(&self) -> Option<TokenKind> {
        self.0.peek_kind()
    }
    pub fn skip_newlines(&mut self) {
        self.0.skip_newlines();
    }
    pub fn parse_module(&mut self) -> crate::error::Result<Module> {
        self.0.parse_module()
    }
    pub fn parse_stmt(&mut self) -> crate::error::Result<Spanned<Stmt>> {
        self.0.parse_stmt()
    }
    pub fn parse_expr(&mut self) -> crate::error::Result<Spanned<Expr>> {
        self.0.parse_expr()
    }
    pub fn parse_tuple_or_expr(&mut self) -> crate::error::Result<Spanned<Expr>> {
        self.0.parse_tuple_or_expr()
    }
    pub fn parse_type_expr(&mut self) -> crate::error::Result<Spanned<TypeExpr>> {
        self.0.parse_type_expr()
    }
    pub fn parse_pattern(&mut self) -> crate::error::Result<Spanned<Pattern>> {
        self.0.parse_pattern()
    }
    pub fn parse_block(&mut self) -> crate::error::Result<Vec<Spanned<Stmt>>> {
        self.0.parse_block()
    }
}

/// Convenience function: parse source into a Module.
pub fn parse(source: &str, file_id: FileId) -> crate::error::Result<Module> {
    let tokens = crate::lexer::lex(source, file_id);
    let mut parser = Parser::new(tokens, source, file_id);
    let mut module = parser.parse_module()?;
    // PEP-classic private name mangling — rewrite `__name` inside class bodies
    // before the type checker and lowering observe the AST.
    mangle_private_names(&mut module);
    Ok(module)
}

/// Apply the same post-parse private-name mangling as [`parse`] to ASTs built
/// by lower-level parser entrypoints such as exec()/compile().
pub fn mangle_private_names(module: &mut Module) {
    crate::domain::parser::mangle::mangle_module(module);
}

pub use mamba_shared_kernel::ast;
