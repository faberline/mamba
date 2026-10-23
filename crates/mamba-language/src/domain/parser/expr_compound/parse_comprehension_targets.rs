use super::*;

impl<'a> Parser<'a> {
    pub(super) fn parse_comprehension_targets(
        &mut self,
    ) -> crate::error::Result<(Vec<String>, bool, Vec<String>)> {
        let mut targets = Vec::new();
        let mut target_reads = Vec::new();
        let unpack_target = self.parse_comprehension_target_list_until(
            TokenKind::In,
            &mut targets,
            &mut target_reads,
        )?;
        if targets.is_empty() {
            let span = self
                .peek()
                .map(|token| Span::new(self.file_id, token.start, token.end))
                .unwrap_or_else(Span::dummy);
            return Err(crate::error::MambaError::syntax(
                span,
                "expected comprehension target",
            ));
        }
        let target_reads_before_bind = target_reads
            .into_iter()
            .filter(|name| targets.iter().any(|target| target == name))
            .collect();
        Ok((targets, unpack_target, target_reads_before_bind))
    }
}
