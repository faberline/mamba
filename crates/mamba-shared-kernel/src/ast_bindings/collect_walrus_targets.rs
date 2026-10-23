use super::*;

/// Scan an expression for walrus targets. Walrus `x := expr` names are
/// locals of the enclosing function (PEP 572), so the assignment-target
/// pre-scan must visit every expression position in the body — including
/// conditions, return values, comprehensions — not just Assign statement
/// targets.
pub fn collect_walrus_targets(expr: &Expr, assigned: &mut Vec<String>) {
    match expr {
        Expr::Walrus { target, value } => {
            if !assigned.iter().any(|n| n == target) {
                assigned.push(target.clone());
            }
            collect_walrus_targets(&value.node, assigned);
        }
        Expr::BinOp { lhs, rhs, .. } => {
            collect_walrus_targets(&lhs.node, assigned);
            collect_walrus_targets(&rhs.node, assigned);
        }
        Expr::UnaryOp { operand, .. } => collect_walrus_targets(&operand.node, assigned),
        Expr::Call { func, args } => {
            collect_walrus_targets(&func.node, assigned);
            for arg in args {
                match arg {
                    CallArg::Positional(e) | CallArg::StarArg(e) | CallArg::DoubleStarArg(e) => {
                        collect_walrus_targets(&e.node, assigned)
                    }
                    CallArg::Keyword { value, .. } => collect_walrus_targets(&value.node, assigned),
                }
            }
        }
        Expr::Attr { object, .. } => collect_walrus_targets(&object.node, assigned),
        Expr::Index { object, index } => {
            collect_walrus_targets(&object.node, assigned);
            collect_walrus_targets(&index.node, assigned);
        }
        Expr::Slice { start, stop, step } => {
            if let Some(s) = start {
                collect_walrus_targets(&s.node, assigned);
            }
            if let Some(s) = stop {
                collect_walrus_targets(&s.node, assigned);
            }
            if let Some(s) = step {
                collect_walrus_targets(&s.node, assigned);
            }
        }
        Expr::TupleLit(elems)
        | Expr::ListLit(elems)
        | Expr::SetLit(elems)
        | Expr::UnpackTarget(elems) => {
            for e in elems {
                collect_walrus_targets(&e.node, assigned);
            }
        }
        Expr::DictLit(entries) => {
            for (key, value) in entries {
                if let Some(key) = key {
                    collect_walrus_targets(&key.node, assigned);
                }
                collect_walrus_targets(&value.node, assigned);
            }
        }
        Expr::IfExpr {
            body,
            condition,
            else_body,
        } => {
            collect_walrus_targets(&condition.node, assigned);
            collect_walrus_targets(&body.node, assigned);
            collect_walrus_targets(&else_body.node, assigned);
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
            collect_walrus_targets(&element.node, assigned);
            for generator in generators {
                collect_walrus_targets(&generator.iter.node, assigned);
                for condition in &generator.conditions {
                    collect_walrus_targets(&condition.node, assigned);
                }
            }
        }
        Expr::DictComp {
            key,
            value,
            generators,
        } => {
            collect_walrus_targets(&key.node, assigned);
            collect_walrus_targets(&value.node, assigned);
            for generator in generators {
                collect_walrus_targets(&generator.iter.node, assigned);
                for condition in &generator.conditions {
                    collect_walrus_targets(&condition.node, assigned);
                }
            }
        }
        Expr::FString(parts) => {
            fn visit_parts(parts: &[FStringPart], assigned: &mut Vec<String>) {
                for part in parts {
                    if let FStringPart::Expr(expr, spec) = part {
                        collect_walrus_targets(&expr.node, assigned);
                        if let Some(spec) = spec {
                            visit_parts(spec, assigned);
                        }
                    }
                }
            }
            visit_parts(parts, assigned);
        }
        Expr::Yield(Some(value)) => collect_walrus_targets(&value.node, assigned),
        Expr::YieldFrom(value) | Expr::Await(value) => {
            collect_walrus_targets(&value.node, assigned)
        }
        Expr::ChainedCompare { operands, .. } => {
            for o in operands {
                collect_walrus_targets(&o.node, assigned);
            }
        }
        Expr::Starred(inner) => collect_walrus_targets(&inner.node, assigned),
        Expr::Lambda { params, .. } => {
            for param in params {
                if let Some(default) = &param.default {
                    collect_walrus_targets(&default.node, assigned);
                }
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
        | Expr::Ident(_)
        | Expr::Yield(None) => {}
    }
}
