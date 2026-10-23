use super::*;

impl<'a> Parser<'a> {
    pub(super) fn parse_comprehension_target_list_until(
        &mut self,
        stop: TokenKind,
        targets: &mut Vec<String>,
        target_reads: &mut Vec<String>,
    ) -> crate::error::Result<bool> {
        let mut had_comma = false;
        while self.peek_kind() != Some(stop.clone()) {
            had_comma |= self.parse_comprehension_target_atom(&stop, targets, target_reads)?;
            if self.peek_kind() != Some(TokenKind::Comma) {
                break;
            }
            had_comma = true;
            self.advance();
            if self.peek_kind() == Some(stop.clone()) {
                break;
            }
        }
        Ok(had_comma)
    }
}
