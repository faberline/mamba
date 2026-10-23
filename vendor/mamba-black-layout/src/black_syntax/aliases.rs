//! Parse complete alias expressions before projecting the exception header.
use super::*;

pub(super) fn collect(source: &str, tokens: &Tokens) -> Vec<Alias> {
    let tokens: Vec<_> = tokens.iter().collect();
    let mut aliases = Vec::new();
    let mut header = false;
    let mut depth = 0u32;
    for (index, token) in tokens.iter().enumerate() {
        match token.kind() {
            TokenKind::Except => {
                header = true;
                depth = 0;
            }
            TokenKind::As if header && depth == 0 => {
                if let Some(alias) = alias(source, &tokens[index + 1..]) {
                    aliases.push(alias);
                }
                header = false;
            }
            TokenKind::Colon | TokenKind::Newline if depth == 0 => header = false,
            TokenKind::Lpar | TokenKind::Lsqb | TokenKind::Lbrace if header => depth += 1,
            TokenKind::Rpar | TokenKind::Rsqb | TokenKind::Rbrace if header => {
                depth = depth.saturating_sub(1)
            }
            _ => {}
        }
    }
    aliases
}

fn alias(source: &str, tokens: &[&ruff_python_ast::token::Token]) -> Option<Alias> {
    let first = tokens
        .iter()
        .find(|token| !token.kind().is_comment() && !token.kind().is_any_newline())?;
    let start = first.start();
    let mut depth = 0u32;
    for token in tokens {
        match token.kind() {
            TokenKind::Lpar | TokenKind::Lsqb | TokenKind::Lbrace => depth += 1,
            TokenKind::Rpar | TokenKind::Rsqb | TokenKind::Rbrace => {
                depth = depth.checked_sub(1)?
            }
            TokenKind::Newline if depth == 0 => return None,
            TokenKind::Colon if depth == 0 => {
                let end = tokens
                    .iter()
                    .take_while(|prior| prior.start() < token.start())
                    .filter(|prior| !prior.kind().is_comment() && !prior.kind().is_any_newline())
                    .last()?
                    .end();
                let range = TextRange::new(start, end);
                let Some(expression) = expression(source, range) else {
                    continue;
                };
                let expr = &expression;
                if expr
                    .as_tuple_expr()
                    .is_some_and(|tuple| !tuple.parenthesized)
                {
                    return None;
                }
                // Plain names already have the complete standard AST representation.
                if expr.is_name_expr() && expr.range() == range {
                    return None;
                }
                return Some(Alias { range, expression });
            }
            _ => {}
        }
    }
    None
}

fn expression(source: &str, range: TextRange) -> Option<Expr> {
    match parse_expression_range(source, range) {
        Ok(parsed) => Some(parsed.into_expr()),
        Err(error) if is_permissive_target_error(&error.error) => {
            // Parenthesized expression mode ignores the padding. Keep byte offsets
            // so tokens, comments, and source parentheses still refer to the source.
            let mut projection = source.as_bytes()[..range.end().to_usize()].to_vec();
            projection[..range.start().to_usize()].fill(b' ');
            let parsed = parse_unchecked(
                std::str::from_utf8(&projection).ok()?,
                ParseOptions::from(Mode::ParenthesizedExpression),
            );
            if parsed
                .errors()
                .iter()
                .any(|error| !is_permissive_target_error(&error.error))
            {
                return None;
            }
            match parsed.into_syntax() {
                Mod::Expression(expression) => Some(*expression.body),
                Mod::Module(_) => None,
            }
        }
        Err(_) => None,
    }
}
