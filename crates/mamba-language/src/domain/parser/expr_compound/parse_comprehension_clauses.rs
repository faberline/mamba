use super::*;

/// Parse comprehension clauses: `for x in iter if cond ...`
///
/// Supports both single-variable (`for x in …`) and tuple-target
/// (`for k, v in …`) loop variables.
impl<'a> Parser<'a> {
    pub(crate) fn parse_comprehension_clauses(
        &mut self,
    ) -> crate::error::Result<Vec<Comprehension>> {
        let mut generators = Vec::new();
        while self.peek_kind() == Some(TokenKind::For) || self.peek_kind() == Some(TokenKind::Async)
        {
            let is_async = if self.peek_kind() == Some(TokenKind::Async) {
                self.advance();
                true
            } else {
                false
            };
            self.expect(TokenKind::For)?;

            let (targets, unpack_target, target_reads_before_bind) =
                self.parse_comprehension_targets()?;

            self.expect(TokenKind::In)?;
            let iter = self.parse_expr_bp(0)?;
            let mut conditions = Vec::new();
            while self.peek_kind() == Some(TokenKind::If) {
                self.advance();
                conditions.push(self.parse_expr_bp(0)?);
            }
            generators.push(Comprehension {
                targets,
                unpack_target,
                target_reads_before_bind,
                iter,
                conditions,
                is_async,
            });
        }
        Ok(generators)
    }
}
