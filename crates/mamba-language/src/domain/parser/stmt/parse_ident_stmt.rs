use super::*;

/// Parse `ident: type = expr` / `ident = expr` / `ident += expr` / expr stmt.
impl<'a> Parser<'a> {
    pub(super) fn parse_ident_stmt(&mut self) -> crate::error::Result<Spanned<Stmt>> {
        let start = self.peek().unwrap().start;
        // This expression is at the top of a statement: a bare walrus here is a
        // SyntaxError. parse_expr captures-and-clears the flag, so nested and
        // parenthesized sub-expressions are unaffected.
        self.stmt_expr_toplevel = true;
        let expr = self.parse_expr()?;

        // Variable declaration `name: type = expr`
        // or bare annotation   `name: type`           (no value, #1014)
        // Also handles attribute annotations: `self.x: int = 0` (PEP 526)
        if self.peek_kind() == Some(TokenKind::Colon) {
            if let Expr::Ident(name) = &expr.node {
                let name = name.clone();
                self.advance();
                let ty = self.parse_type_expr()?;
                if self.peek_kind() == Some(TokenKind::Eq) {
                    self.advance();
                    let value = self.parse_expr()?;
                    self.skip_newlines();
                    return Ok(Spanned::new(
                        Stmt::VarDecl { name, ty, value },
                        self.span_from(start),
                    ));
                } else {
                    // Bare annotation: `id: int` (no default value)
                    self.skip_newlines();
                    return Ok(Spanned::new(
                        Stmt::BareAnnotation { name, ty },
                        self.span_from(start),
                    ));
                }
            } else {
                // Non-simple annotation target: `self.x: int = 0` or `obj[key]: T = v`
                // Consume and discard the annotation; emit as assignment if `= value` follows.
                self.advance(); // consume `:`
                let _ty = self.parse_type_expr()?;
                if self.peek_kind() == Some(TokenKind::Eq) {
                    self.advance();
                    let value = self.parse_expr()?;
                    self.skip_newlines();
                    return Ok(Spanned::new(
                        Stmt::Assign {
                            target: expr,
                            value,
                        },
                        self.span_from(start),
                    ));
                } else {
                    // Bare attr annotation: `self.x: int`
                    self.skip_newlines();
                    return Ok(Spanned::new(Stmt::ExprStmt(expr), self.span_from(start)));
                }
            }
        }

        // Tuple unpacking: `a, b, ... = expr` (also chained: `a, b = c = expr`)
        if self.peek_kind() == Some(TokenKind::Comma) {
            let mut tuple_elems = vec![expr];
            while self.peek_kind() == Some(TokenKind::Comma) {
                self.advance();
                if self.peek_kind() == Some(TokenKind::Eq) {
                    break;
                }
                tuple_elems.push(self.parse_expr()?);
            }
            if self.peek_kind() == Some(TokenKind::Eq) {
                self.advance();
                let span = self.span_from(start);
                // List displays among the comma-separated targets are
                // sequence-unpack targets (`[a, b], c = ...`); normalize them.
                let tuple_elems: Vec<_> = tuple_elems
                    .into_iter()
                    .map(Self::normalize_assign_target)
                    .collect();
                let tuple_target = Spanned::new(Expr::TupleLit(tuple_elems), span);
                let mut value = self.parse_tuple_or_expr()?;
                // Chained: `a, b = c = val` — reuse the simple-chain desugar
                // (one __chained_N__ temp, share across targets).
                let mut targets: Vec<Spanned<Expr>> = vec![tuple_target];
                while self.peek_kind() == Some(TokenKind::Eq) {
                    self.advance();
                    // Intermediate chained target may itself be a list display.
                    targets.push(Self::normalize_assign_target(value));
                    value = self.parse_tuple_or_expr()?;
                }
                Self::validate_assign_targets(&targets, "cannot assign to expression")?;
                self.skip_newlines();
                if targets.len() == 1 {
                    return Ok(Spanned::new(
                        Stmt::Assign {
                            target: targets.into_iter().next().unwrap(),
                            value,
                        },
                        span,
                    ));
                }
                let tmp_name = format!("__chained_{}__", start);
                let tmp_ident = Spanned::new(Expr::Ident(tmp_name), span);
                let mut all_stmts: Vec<Spanned<Stmt>> = Vec::with_capacity(targets.len() + 1);
                all_stmts.push(Spanned::new(
                    Stmt::Assign {
                        target: tmp_ident.clone(),
                        value,
                    },
                    span,
                ));
                for target in targets.into_iter() {
                    all_stmts.push(Spanned::new(
                        Stmt::Assign {
                            target,
                            value: tmp_ident.clone(),
                        },
                        span,
                    ));
                }
                let first = all_stmts.remove(0);
                all_stmts.reverse();
                self.pending_stmts.extend(all_stmts);
                return Ok(first);
            }
            // Not assignment — tuple expression statement
            self.skip_newlines();
            let span = self.span_from(start);
            let tuple = Spanned::new(Expr::TupleLit(tuple_elems), span);
            return Ok(Spanned::new(Stmt::ExprStmt(tuple), span));
        }

        // Augmented assignment: `target op= expr`
        if let Some(aug_op) = self.peek_aug_op() {
            Self::validate_assign_target(&expr, "illegal expression for augmented assignment")?;
            self.advance();
            let value = self.parse_tuple_or_expr()?;
            self.skip_newlines();
            return Ok(Spanned::new(
                Stmt::AugAssign {
                    target: expr,
                    op: aug_op,
                    value,
                },
                self.span_from(start),
            ));
        }

        // Assignment: `target = expr` or `a = b = val` (chained)
        if self.peek_kind() == Some(TokenKind::Eq) {
            self.advance();
            let mut value = self.parse_tuple_or_expr()?;
            // Chained assignment: `a = b = val` → emit as `a = val`, ignoring middle targets
            // We keep consuming `= expr` until no more `=`.
            let mut targets = vec![expr];
            while self.peek_kind() == Some(TokenKind::Eq) {
                self.advance();
                targets.push(value);
                value = self.parse_tuple_or_expr()?;
            }
            // A list display in target position (`[*a, b] = xs`) is a
            // sequence-unpack target; normalize every target to a tuple form.
            let targets: Vec<_> = targets
                .into_iter()
                .map(Self::normalize_assign_target)
                .collect();
            Self::validate_assign_targets(&targets, "cannot assign to expression")?;
            self.skip_newlines();
            let span = self.span_from(start);
            if targets.len() == 1 {
                return Ok(Spanned::new(
                    Stmt::Assign {
                        target: targets.into_iter().next().unwrap(),
                        value,
                    },
                    span,
                ));
            }
            // `a = b = c = val` — CPython semantics evaluate RHS once and
            // share it across every target (so `a.append(x)` mutates what
            // `b` / `c` see). Desugar into:
            //
            //   __chained_N = val
            //   a = __chained_N
            //   b = __chained_N
            //   c = __chained_N
            //
            // so side effects in `val` happen once and mutable references
            // stay shared. The temp uses the statement's byte offset as a
            // suffix to stay unique across nested scopes.
            let tmp_name = format!("__chained_{}__", start);
            let tmp_ident = Spanned::new(Expr::Ident(tmp_name.clone()), span);
            let mut all_stmts: Vec<Spanned<Stmt>> = Vec::with_capacity(targets.len() + 1);
            all_stmts.push(Spanned::new(
                Stmt::Assign {
                    target: tmp_ident.clone(),
                    value,
                },
                span,
            ));
            for target in targets.into_iter() {
                all_stmts.push(Spanned::new(
                    Stmt::Assign {
                        target,
                        value: tmp_ident.clone(),
                    },
                    span,
                ));
            }
            // Return the first (temp assignment); the rest go in the pending
            // buffer, reversed so pop() yields source order.
            let first = all_stmts.remove(0);
            all_stmts.reverse();
            self.pending_stmts.extend(all_stmts);
            return Ok(first);
        }

        // Expression statement
        self.skip_newlines();
        let span = expr.span;
        Ok(Spanned::new(Stmt::ExprStmt(expr), span))
    }
}
