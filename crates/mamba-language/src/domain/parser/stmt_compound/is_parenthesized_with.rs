use super::*;

/// Compound statement parsers: def, class, enum, if, while, for, match,
/// try, raise, with, assert, del, global, nonlocal, type alias, decorated.
/// Lookahead to determine if a `(` after `with` introduces a parenthesized
/// context-manager list (PEP 617) vs a parenthesized expression.
///
/// We scan forward past the opening `(` to find either:
///   - A bare expression followed by `as`, `,`, or `)` → parenthesized with
///   - A `)` followed by `:` → empty parenthesized (unlikely but valid)
///   - A complex expression that looks like a single context manager → fall through
impl<'a> Parser<'a> {
    pub(super) fn is_parenthesized_with(&self) -> bool {
        // Called after `with` has already been consumed, so `self.pos` is at `(`.
        // Scan forward past the `(` and look for `as` or `,` at depth 1 before `:` or `)`.
        let mut i = self.pos; // pos is at `(`
                              // skip `(`
        if i >= self.tokens.len() || self.tokens[i].kind != TokenKind::LParen {
            return false;
        }
        i += 1;
        let mut depth = 1i32;
        while i < self.tokens.len() {
            match &self.tokens[i].kind {
                TokenKind::LParen | TokenKind::LBracket | TokenKind::LBrace => {
                    depth += 1;
                }
                TokenKind::RParen | TokenKind::RBracket | TokenKind::RBrace => {
                    depth -= 1;
                    if depth == 0 {
                        // Closed the outer paren. Check if `:` follows.
                        let next = i + 1;
                        if next < self.tokens.len() {
                            match &self.tokens[next].kind {
                                TokenKind::Colon | TokenKind::Newline => return true,
                                _ => {}
                            }
                        }
                        return false;
                    }
                }
                // `as` at depth 1 → definitely a parenthesized `with`
                TokenKind::As if depth == 1 => return true,
                // `,` at depth 1 → could be multiple context managers
                TokenKind::Comma if depth == 1 => return true,
                _ => {}
            }
            i += 1;
        }
        false
    }
}
