use super::*;
use crate::application::parser::parse;
use crate::source::span::FileId;

fn fid() -> FileId {
    FileId(0)
}

// --- Parser::new and basic token navigation ---

#[test]
fn test_parse_empty_source() {
    let module = parse("", fid()).unwrap();
    assert!(module.stmts.is_empty());
}

#[test]
fn test_parse_only_newlines() {
    let module = parse("\n\n\n", fid()).unwrap();
    assert!(module.stmts.is_empty());
}

#[test]
fn test_parse_single_pass() {
    let module = parse("pass\n", fid()).unwrap();
    assert_eq!(module.stmts.len(), 1);
    assert!(matches!(&module.stmts[0].node, ast::Stmt::Pass));
}

#[test]
fn test_parse_multiple_statements() {
    let module = parse("pass\nbreak\ncontinue\n", fid()).unwrap();
    assert_eq!(module.stmts.len(), 3);
    assert!(matches!(&module.stmts[0].node, ast::Stmt::Pass));
    assert!(matches!(&module.stmts[1].node, ast::Stmt::Break));
    assert!(matches!(&module.stmts[2].node, ast::Stmt::Continue));
}

// --- peek / peek_kind / advance ---

#[test]
fn test_peek_returns_first_token() {
    let tokens = crate::lexer::lex("42\n", fid());
    let parser = Parser::new(tokens, "42\n", fid(), crate::app::RAW_LEXER);
    assert!(parser.peek().is_some());
    assert!(matches!(parser.peek_kind(), Some(TokenKind::Int(42))));
}

#[test]
fn test_advance_moves_position() {
    let tokens = crate::lexer::lex("42\n", fid());
    let mut parser = Parser::new(tokens, "42\n", fid(), crate::app::RAW_LEXER);
    let (start, end) = parser.advance();
    assert_eq!(start, 0);
    assert_eq!(end, 2);
    // After advancing past Int(42), next should be Newline or Eof
    assert!(parser.peek().is_some());
}

// --- expect ---

#[test]
fn test_expect_success() {
    let src = "pass\n";
    let tokens = crate::lexer::lex(src, fid());
    let mut parser = Parser::new(tokens, src, fid(), crate::app::RAW_LEXER);
    assert!(parser.expect(TokenKind::Pass).is_ok());
}

#[test]
fn test_expect_wrong_token() {
    let src = "pass\n";
    let tokens = crate::lexer::lex(src, fid());
    let mut parser = Parser::new(tokens, src, fid(), crate::app::RAW_LEXER);
    let err = parser.expect(TokenKind::Break).unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("expected"), "error message: {msg}");
}

// --- expect_name ---

#[test]
fn test_expect_name_ident() {
    let src = "foo\n";
    let tokens = crate::lexer::lex(src, fid());
    let mut parser = Parser::new(tokens, src, fid(), crate::app::RAW_LEXER);
    let (s, e) = parser.expect_name().unwrap();
    assert_eq!(parser.text_at(s, e), "foo");
}

#[test]
fn test_expect_name_unicode_ident() {
    let src = "áóí = 20\n";
    let tokens = crate::lexer::lex(src, fid());
    let mut parser = Parser::new(tokens, src, fid(), crate::app::RAW_LEXER);
    let (s, e) = parser.expect_name().unwrap();
    assert_eq!(parser.text_at(s, e), "áóí");
}

#[test]
fn test_expect_name_mixed_unicode_ident() {
    let src = "xá_1 = 20\n";
    let tokens = crate::lexer::lex(src, fid());
    let mut parser = Parser::new(tokens, src, fid(), crate::app::RAW_LEXER);
    let (s, e) = parser.expect_name().unwrap();
    assert_eq!(parser.text_at(s, e), "xá_1");
}

#[test]
fn test_expect_name_soft_keyword() {
    // `match` and `enum` are soft keywords usable as names
    let src = "match\n";
    let tokens = crate::lexer::lex(src, fid());
    let mut parser = Parser::new(tokens, src, fid(), crate::app::RAW_LEXER);
    assert!(parser.expect_name().is_ok());
}

#[test]
fn test_expect_name_rejects_hard_keyword() {
    let src = "if\n";
    let tokens = crate::lexer::lex(src, fid());
    let mut parser = Parser::new(tokens, src, fid(), crate::app::RAW_LEXER);
    assert!(parser.expect_name().is_err());
}

// --- is_name_token ---

#[test]
fn test_is_name_token_variants() {
    assert!(Parser::is_name_token(&TokenKind::Ident));
    assert!(Parser::is_name_token(&TokenKind::Self_));
    assert!(Parser::is_name_token(&TokenKind::IntType));
    assert!(Parser::is_name_token(&TokenKind::Match));
    assert!(Parser::is_name_token(&TokenKind::Enum));
    assert!(!Parser::is_name_token(&TokenKind::If));
    assert!(!Parser::is_name_token(&TokenKind::Def));
    assert!(!Parser::is_name_token(&TokenKind::Class));
}

// --- skip_newlines ---

#[test]
fn test_skip_newlines() {
    let src = "\n\n\npass\n";
    let tokens = crate::lexer::lex(src, fid());
    let mut parser = Parser::new(tokens, src, fid(), crate::app::RAW_LEXER);
    parser.skip_newlines();
    assert!(matches!(parser.peek_kind(), Some(TokenKind::Pass)));
}

// --- span_from ---

#[test]
fn test_span_from() {
    let src = "x = 1\n";
    let tokens = crate::lexer::lex(src, fid());
    let mut parser = Parser::new(tokens, src, fid(), crate::app::RAW_LEXER);
    parser.advance(); // x
    parser.advance(); // =
    parser.advance(); // 1
    let span = parser.span_from(0);
    assert_eq!(span.start, 0);
    assert_eq!(span.end, 5); // end of `1`
}

// --- parse convenience function error ---

#[test]
fn test_parse_syntax_error() {
    let result = parse("def\n", fid());
    assert!(result.is_err());
}

#[test]
fn test_parse_unicode_ident_assignment_and_use() {
    let module = parse(
        "def f():\n    áóí = 20\n    return 10 + áóí / 0 + 30\n",
        fid(),
    )
    .expect("unicode identifier should parse");
    assert_eq!(module.stmts.len(), 1);

    let body = match &module.stmts[0].node {
        ast::Stmt::FnDef { body, .. } => body,
        other => panic!("expected function definition, got {other:?}"),
    };
    assert_eq!(body.len(), 2);

    match &body[0].node {
        ast::Stmt::Assign { target, value } => {
            assert!(matches!(&target.node, ast::Expr::Ident(name) if name == "áóí"));
            assert!(matches!(&value.node, ast::Expr::IntLit(20)));
        }
        other => panic!("expected assignment, got {other:?}"),
    }

    match &body[1].node {
        ast::Stmt::Return(Some(expr)) => {
            assert!(expr_contains_ident(&expr.node, "áóí"));
        }
        other => panic!("expected return statement, got {other:?}"),
    }
}

fn expr_contains_ident(expr: &ast::Expr, want: &str) -> bool {
    match expr {
        ast::Expr::Ident(name) => name == want,
        ast::Expr::BinOp { lhs, rhs, .. } => {
            expr_contains_ident(&lhs.node, want) || expr_contains_ident(&rhs.node, want)
        }
        _ => false,
    }
}

// --- text_at ---

#[test]
fn test_text_at() {
    let src = "hello world\n";
    let tokens = crate::lexer::lex(src, fid());
    let parser = Parser::new(tokens, src, fid(), crate::app::RAW_LEXER);
    assert_eq!(parser.text_at(0, 5), "hello");
    assert_eq!(parser.text_at(6, 11), "world");
}
