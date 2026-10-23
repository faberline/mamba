use super::*;

#[test]
fn test_indent_processor_initial_state() {
    let proc = IndentProcessor::new();
    assert_eq!(proc.indent_stack, vec![0]);
    assert!(proc.at_line_start);
    assert_eq!(proc.paren_depth, 0);

    let proc_def = IndentProcessor::default();
    assert_eq!(proc_def.indent_stack, vec![0]);
}

#[test]
fn test_indent_dedent_injection() {
    // Raw tokens representing:
    // x = 1\n
    //   y = 2\n
    // z = 3
    let raw = vec![
        Token::new(TokenKind::Ident, 0, 1),
        Token::new(TokenKind::Eq, 2, 3),
        Token::new(TokenKind::Int(1), 4, 5),
        Token::new(TokenKind::Newline, 5, 6),
        Token::new(TokenKind::Ident, 8, 9), // start 8, newline end 6 => indent 2
        Token::new(TokenKind::Eq, 10, 11),
        Token::new(TokenKind::Int(2), 12, 13),
        Token::new(TokenKind::Newline, 13, 14),
        Token::new(TokenKind::Ident, 14, 15), // start 14, newline end 14 => indent 0
        Token::new(TokenKind::Eq, 16, 17),
        Token::new(TokenKind::Int(3), 18, 19),
    ];

    let mut processor = IndentProcessor::new();
    let tokens = processor.process(raw);

    let kinds: Vec<TokenKind> = tokens.into_iter().map(|t| t.kind).collect();
    assert!(kinds.contains(&TokenKind::Indent));
    assert!(kinds.contains(&TokenKind::Dedent));
    assert_eq!(kinds.last(), Some(&TokenKind::Eof));
}

#[test]
fn test_newline_suppression_inside_parens() {
    // Raw tokens: ( \n 1 \n )
    let raw = vec![
        Token::new(TokenKind::LParen, 0, 1),
        Token::new(TokenKind::Newline, 1, 2),
        Token::new(TokenKind::Int(1), 3, 4),
        Token::new(TokenKind::Newline, 4, 5),
        Token::new(TokenKind::RParen, 5, 6),
    ];

    let mut processor = IndentProcessor::new();
    let tokens = processor.process(raw);
    let kinds: Vec<TokenKind> = tokens.into_iter().map(|t| t.kind).collect();

    // Newlines inside parens should be skipped
    assert!(!kinds.contains(&TokenKind::Newline));
    assert!(!kinds.contains(&TokenKind::Indent));
}
