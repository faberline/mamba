use super::*;

impl<'a> Parser<'a> {
    pub(crate) fn parse_expr_bp(&mut self, min_bp: u8) -> crate::error::Result<Spanned<Expr>> {
        let mut lhs = self.parse_prefix()?;

        loop {
            // Postfix operations (call, attr, index/slice) — loop to handle chains
            loop {
                lhs = match self.peek_kind() {
                    Some(TokenKind::LParen) => self.parse_call(lhs)?,
                    Some(TokenKind::Dot) => self.parse_attr(lhs)?,
                    Some(TokenKind::LBracket) => self.parse_index_or_slice(lhs)?,
                    _ => break,
                };
            }

            // Infix operations
            let op = match self.peek_kind() {
                Some(ref k) => match self.token_to_binop(k) {
                    Some(op) => op,
                    None => break,
                },
                None => break,
            };

            let (l_bp, r_bp) = infix_bp(&op);
            if l_bp < min_bp {
                break;
            }

            self.advance_binop(&op);
            let rhs = self.parse_expr_bp(r_bp)?;

            // Chained comparison: if this operator is a comparison and the next
            // token is also a comparison operator, collect into ChainedCompare.
            if is_comparison_op(&op) {
                if let Some(ref next_kind) = self.peek_kind() {
                    if let Some(next_op) = self.token_to_binop(next_kind) {
                        if is_comparison_op(&next_op) {
                            let (next_l_bp, _) = infix_bp(&next_op);
                            if next_l_bp >= min_bp {
                                // Start collecting a chain: operands=[lhs, rhs, ...], ops=[op, ...]
                                let mut operands = vec![lhs, rhs];
                                let mut ops = vec![op];
                                while let Some(ref k) = self.peek_kind() {
                                    let chain_op = match self.token_to_binop(k) {
                                        Some(o) if is_comparison_op(&o) => o,
                                        _ => break,
                                    };
                                    let (chain_l_bp, chain_r_bp) = infix_bp(&chain_op);
                                    if chain_l_bp < min_bp {
                                        break;
                                    }
                                    self.advance_binop(&chain_op);
                                    let next_rhs = self.parse_expr_bp(chain_r_bp)?;
                                    ops.push(chain_op);
                                    operands.push(next_rhs);
                                }
                                let start = operands.first().unwrap().span;
                                let end = operands.last().unwrap().span;
                                let span = start.merge(end);
                                lhs = Spanned::new(Expr::ChainedCompare { operands, ops }, span);
                                continue;
                            }
                        }
                    }
                }
            }

            let span = lhs.span.merge(rhs.span);
            lhs = Spanned::new(
                Expr::BinOp {
                    op,
                    lhs: Box::new(lhs),
                    rhs: Box::new(rhs),
                },
                span,
            );
        }

        Ok(lhs)
    }
}
