use super::*;

impl<'a> Parser<'a> {
    pub(crate) fn validate_pep695_expr(
        &self,
        expr: &Spanned<Expr>,
        context: Pep695ExprContext,
    ) -> crate::error::Result<()> {
        match &expr.node {
            Expr::Yield(_) | Expr::YieldFrom(_) | Expr::Await(_) | Expr::Walrus { .. } => {
                return Err(MambaError::syntax(
                    expr.span,
                    context.syntax_message(&expr.node).to_string(),
                ));
            }
            Expr::BinOp { lhs, rhs, .. } => {
                self.validate_pep695_expr(lhs, context)?;
                self.validate_pep695_expr(rhs, context)?;
            }
            Expr::UnaryOp { operand, .. } | Expr::Starred(operand) => {
                self.validate_pep695_expr(operand, context)?;
            }
            Expr::Call { func, args } => {
                self.validate_pep695_expr(func, context)?;
                for arg in args {
                    match arg {
                        CallArg::Positional(value)
                        | CallArg::StarArg(value)
                        | CallArg::DoubleStarArg(value)
                        | CallArg::Keyword { value, .. } => {
                            self.validate_pep695_expr(value, context)?;
                        }
                    }
                }
            }
            Expr::Attr { object, .. } => {
                self.validate_pep695_expr(object, context)?;
            }
            Expr::Index { object, index } => {
                self.validate_pep695_expr(object, context)?;
                self.validate_pep695_expr(index, context)?;
            }
            Expr::Slice { start, stop, step } => {
                for part in [start, stop, step].into_iter().flatten() {
                    self.validate_pep695_expr(part, context)?;
                }
            }
            Expr::ListLit(items) | Expr::SetLit(items) | Expr::TupleLit(items) => {
                for item in items {
                    self.validate_pep695_expr(item, context)?;
                }
            }
            Expr::DictLit(entries) => {
                for (key, value) in entries {
                    if let Some(key) = key {
                        self.validate_pep695_expr(key, context)?;
                    }
                    self.validate_pep695_expr(value, context)?;
                }
            }
            Expr::IfExpr {
                body,
                condition,
                else_body,
            } => {
                self.validate_pep695_expr(body, context)?;
                self.validate_pep695_expr(condition, context)?;
                self.validate_pep695_expr(else_body, context)?;
            }
            Expr::Lambda { params, body } => {
                for param in params {
                    if let Some(default) = &param.default {
                        self.validate_pep695_expr(default, context)?;
                    }
                }
                self.validate_pep695_expr(body, context)?;
            }
            Expr::ListComp {
                element,
                generators,
            }
            | Expr::SetComp {
                element,
                generators,
            }
            | Expr::GeneratorExpr {
                element,
                generators,
            } => {
                for generator in generators {
                    self.validate_pep695_expr(&generator.iter, context)?;
                    for condition in &generator.conditions {
                        self.validate_pep695_expr(condition, context)?;
                    }
                }
                self.validate_pep695_expr(element, context)?;
            }
            Expr::DictComp {
                key,
                value,
                generators,
            } => {
                for generator in generators {
                    self.validate_pep695_expr(&generator.iter, context)?;
                    for condition in &generator.conditions {
                        self.validate_pep695_expr(condition, context)?;
                    }
                }
                self.validate_pep695_expr(key, context)?;
                self.validate_pep695_expr(value, context)?;
            }
            Expr::FString(parts) => {
                for part in parts {
                    self.validate_pep695_fstring_part(part, context)?;
                }
            }
            Expr::ChainedCompare { operands, .. } | Expr::UnpackTarget(operands) => {
                for operand in operands {
                    self.validate_pep695_expr(operand, context)?;
                }
            }
            Expr::IntLit(_)
            | Expr::BigIntLit(_)
            | Expr::FloatLit(_)
            | Expr::ComplexLit(_)
            | Expr::StrLit(_)
            | Expr::BytesLit(_)
            | Expr::BoolLit(_)
            | Expr::NoneLit
            | Expr::Ellipsis
            | Expr::Ident(_) => {}
        }
        Ok(())
    }
}
