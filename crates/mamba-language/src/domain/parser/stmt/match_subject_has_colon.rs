use super::*;

/// Lookahead: starting after the `match` token (pos+1), scan forward over
/// the subject expression (skipping balanced parens/brackets/braces) to
/// determine if there is a `:` at the outermost nesting level.
/// Returns `true` if a `:` is found before a newline/EOF at depth 0.
///
/// A match-statement subject expression is grammatically mandatory
/// (PEP 634), so a `:` found before any subject token has been consumed
/// (#1923) means `match` is being used as a plain identifier — e.g. the
/// annotated-assignment target `match: TYPE` — not a match-statement.
impl<'a> Parser<'a> {
    pub(super) fn match_subject_has_colon(&self) -> bool {
        let mut depth = 0usize;
        let mut i = self.pos + 1; // start after the `match` token
        let mut seen_subject_token = false;
        while i < self.tokens.len() {
            match &self.tokens[i].kind {
                TokenKind::LParen | TokenKind::LBracket | TokenKind::LBrace => {
                    depth += 1;
                }
                TokenKind::RParen | TokenKind::RBracket | TokenKind::RBrace => {
                    if depth == 0 {
                        return false;
                    }
                    depth -= 1;
                }
                TokenKind::Colon if depth == 0 => return seen_subject_token,
                TokenKind::Newline | TokenKind::Eof if depth == 0 => return false,
                _ => {}
            }
            seen_subject_token = true;
            i += 1;
        }
        false
    }
}
