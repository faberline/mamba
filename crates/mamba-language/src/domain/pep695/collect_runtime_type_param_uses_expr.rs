use super::*;

pub(super) fn collect_runtime_type_param_uses_expr(
    expr: &Spanned<Expr>,
    type_param_names: &HashSet<Name>,
    out: &mut HashSet<Name>,
    shadowed: &mut HashSet<Name>,
) {
    match &expr.node {
        Expr::Ident(name) if type_param_names.contains(name) && !shadowed.contains(name) => {
            out.insert(name.clone());
        }
        Expr::BinOp { lhs, rhs, .. } => {
            collect_runtime_type_param_uses_expr(lhs, type_param_names, out, shadowed);
            collect_runtime_type_param_uses_expr(rhs, type_param_names, out, shadowed);
        }
        Expr::UnaryOp { operand, .. }
        | Expr::Starred(operand)
        | Expr::YieldFrom(operand)
        | Expr::Await(operand) => {
            collect_runtime_type_param_uses_expr(operand, type_param_names, out, shadowed);
        }
        Expr::Call { func, args } => {
            collect_runtime_type_param_uses_expr(func, type_param_names, out, shadowed);
            for arg in args {
                match arg {
                    CallArg::Positional(value)
                    | CallArg::StarArg(value)
                    | CallArg::DoubleStarArg(value)
                    | CallArg::Keyword { value, .. } => {
                        collect_runtime_type_param_uses_expr(
                            value,
                            type_param_names,
                            out,
                            shadowed,
                        );
                    }
                }
            }
        }
        Expr::Attr { object, .. } => {
            collect_runtime_type_param_uses_expr(object, type_param_names, out, shadowed);
        }
        Expr::Index { object, index } => {
            collect_runtime_type_param_uses_expr(object, type_param_names, out, shadowed);
            collect_runtime_type_param_uses_expr(index, type_param_names, out, shadowed);
        }
        Expr::Slice { start, stop, step } => {
            for part in [start, stop, step].into_iter().flatten() {
                collect_runtime_type_param_uses_expr(part, type_param_names, out, shadowed);
            }
        }
        Expr::ListLit(items) | Expr::SetLit(items) | Expr::TupleLit(items) => {
            for item in items {
                collect_runtime_type_param_uses_expr(item, type_param_names, out, shadowed);
            }
        }
        Expr::DictLit(entries) => {
            for (key, value) in entries {
                if let Some(key) = key {
                    collect_runtime_type_param_uses_expr(key, type_param_names, out, shadowed);
                }
                collect_runtime_type_param_uses_expr(value, type_param_names, out, shadowed);
            }
        }
        Expr::IfExpr {
            body,
            condition,
            else_body,
        } => {
            collect_runtime_type_param_uses_expr(body, type_param_names, out, shadowed);
            collect_runtime_type_param_uses_expr(condition, type_param_names, out, shadowed);
            collect_runtime_type_param_uses_expr(else_body, type_param_names, out, shadowed);
        }
        Expr::Lambda { params, body } => {
            collect_runtime_type_param_uses_in_params(params, type_param_names, out, shadowed);
            let added: Vec<Name> = params
                .iter()
                .filter_map(|param| {
                    if shadowed.insert(param.name.clone()) {
                        Some(param.name.clone())
                    } else {
                        None
                    }
                })
                .collect();
            collect_runtime_type_param_uses_expr(body, type_param_names, out, shadowed);
            for name in added {
                shadowed.remove(&name);
            }
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
            collect_runtime_type_param_uses_comprehension(
                element,
                generators,
                type_param_names,
                out,
                shadowed,
            );
        }
        Expr::DictComp {
            key,
            value,
            generators,
        } => {
            collect_runtime_type_param_uses_comprehension(
                key,
                generators,
                type_param_names,
                out,
                shadowed,
            );
            collect_runtime_type_param_uses_expr(value, type_param_names, out, shadowed);
        }
        Expr::FString(parts) => {
            for part in parts {
                if let mamba_shared_kernel::ast::FStringPart::Expr(value, format) = part {
                    collect_runtime_type_param_uses_expr(value, type_param_names, out, shadowed);
                    if let Some(format) = format {
                        for part in format {
                            if let mamba_shared_kernel::ast::FStringPart::Expr(value, _) = part {
                                collect_runtime_type_param_uses_expr(
                                    value,
                                    type_param_names,
                                    out,
                                    shadowed,
                                );
                            }
                        }
                    }
                }
            }
        }
        Expr::Yield(value) => {
            if let Some(value) = value {
                collect_runtime_type_param_uses_expr(value, type_param_names, out, shadowed);
            }
        }
        Expr::Walrus { target, value } => {
            collect_runtime_type_param_uses_expr(value, type_param_names, out, shadowed);
            shadowed.insert(target.clone());
        }
        Expr::ChainedCompare { operands, .. } => {
            for operand in operands {
                collect_runtime_type_param_uses_expr(operand, type_param_names, out, shadowed);
            }
        }
        Expr::UnpackTarget(items) => {
            for item in items {
                collect_runtime_type_param_uses_expr(item, type_param_names, out, shadowed);
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
