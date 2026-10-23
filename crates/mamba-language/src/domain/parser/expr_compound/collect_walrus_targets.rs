use super::*;

impl<'a> Parser<'a> {
    pub(super) fn collect_walrus_targets(expr: &Spanned<Expr>, out: &mut Vec<String>) {
        match &expr.node {
            Expr::Walrus { target, value } => {
                out.push(target.clone());
                Self::collect_walrus_targets(value, out);
            }
            Expr::BinOp { lhs, rhs, .. } => {
                Self::collect_walrus_targets(lhs, out);
                Self::collect_walrus_targets(rhs, out);
            }
            Expr::UnaryOp { operand, .. }
            | Expr::Attr {
                object: operand, ..
            } => {
                Self::collect_walrus_targets(operand, out);
            }
            Expr::Call { func, args } => {
                Self::collect_walrus_targets(func, out);
                for arg in args {
                    match arg {
                        CallArg::Positional(value)
                        | CallArg::Keyword { value, .. }
                        | CallArg::StarArg(value)
                        | CallArg::DoubleStarArg(value) => Self::collect_walrus_targets(value, out),
                    }
                }
            }
            Expr::Index { object, index } => {
                Self::collect_walrus_targets(object, out);
                Self::collect_walrus_targets(index, out);
            }
            Expr::Slice { start, stop, step } => {
                if let Some(value) = start {
                    Self::collect_walrus_targets(value, out);
                }
                if let Some(value) = stop {
                    Self::collect_walrus_targets(value, out);
                }
                if let Some(value) = step {
                    Self::collect_walrus_targets(value, out);
                }
            }
            Expr::ListLit(values)
            | Expr::SetLit(values)
            | Expr::TupleLit(values)
            | Expr::UnpackTarget(values) => {
                for value in values {
                    Self::collect_walrus_targets(value, out);
                }
            }
            Expr::DictLit(entries) => {
                for (key, value) in entries {
                    if let Some(key) = key {
                        Self::collect_walrus_targets(key, out);
                    }
                    Self::collect_walrus_targets(value, out);
                }
            }
            Expr::IfExpr {
                body,
                condition,
                else_body,
            } => {
                Self::collect_walrus_targets(body, out);
                Self::collect_walrus_targets(condition, out);
                Self::collect_walrus_targets(else_body, out);
            }
            Expr::Lambda { .. } => {}
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
                Self::collect_walrus_targets(element, out);
                for generator in generators {
                    for condition in &generator.conditions {
                        Self::collect_walrus_targets(condition, out);
                    }
                }
            }
            Expr::DictComp {
                key,
                value,
                generators,
            } => {
                Self::collect_walrus_targets(key, out);
                Self::collect_walrus_targets(value, out);
                for generator in generators {
                    for condition in &generator.conditions {
                        Self::collect_walrus_targets(condition, out);
                    }
                }
            }
            Expr::FString(parts) => {
                for part in parts {
                    Self::collect_walrus_targets_in_fstring_part(part, out);
                }
            }
            Expr::Yield(value) => {
                if let Some(value) = value {
                    Self::collect_walrus_targets(value, out);
                }
            }
            Expr::YieldFrom(value) | Expr::Await(value) | Expr::Starred(value) => {
                Self::collect_walrus_targets(value, out);
            }
            Expr::ChainedCompare { operands, .. } => {
                for operand in operands {
                    Self::collect_walrus_targets(operand, out);
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
    }
}
