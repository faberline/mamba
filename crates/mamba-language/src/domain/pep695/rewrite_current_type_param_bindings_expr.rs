use super::*;

pub(super) fn rewrite_current_type_param_bindings_expr(
    expr: &mut Spanned<Expr>,
    bindings: &HashMap<Name, Name>,
    shadowed: &mut HashSet<Name>,
) {
    match &mut expr.node {
        Expr::Ident(name) if !shadowed.contains(name) => {
            if let Some(binding) = bindings.get(name) {
                *name = binding.clone();
            }
        }
        Expr::BinOp { lhs, rhs, .. } => {
            rewrite_current_type_param_bindings_expr(lhs, bindings, shadowed);
            rewrite_current_type_param_bindings_expr(rhs, bindings, shadowed);
        }
        Expr::UnaryOp { operand, .. }
        | Expr::Starred(operand)
        | Expr::YieldFrom(operand)
        | Expr::Await(operand) => {
            rewrite_current_type_param_bindings_expr(operand, bindings, shadowed);
        }
        Expr::Call { func, args } => {
            rewrite_current_type_param_bindings_expr(func, bindings, shadowed);
            for arg in args {
                match arg {
                    CallArg::Positional(value)
                    | CallArg::StarArg(value)
                    | CallArg::DoubleStarArg(value)
                    | CallArg::Keyword { value, .. } => {
                        rewrite_current_type_param_bindings_expr(value, bindings, shadowed);
                    }
                }
            }
        }
        Expr::Attr { object, .. } => {
            rewrite_current_type_param_bindings_expr(object, bindings, shadowed);
        }
        Expr::Index { object, index } => {
            rewrite_current_type_param_bindings_expr(object, bindings, shadowed);
            rewrite_current_type_param_bindings_expr(index, bindings, shadowed);
        }
        Expr::Slice { start, stop, step } => {
            for part in [start, stop, step].into_iter().flatten() {
                rewrite_current_type_param_bindings_expr(part, bindings, shadowed);
            }
        }
        Expr::ListLit(items)
        | Expr::SetLit(items)
        | Expr::TupleLit(items)
        | Expr::UnpackTarget(items) => {
            for item in items {
                rewrite_current_type_param_bindings_expr(item, bindings, shadowed);
            }
        }
        Expr::DictLit(entries) => {
            for (key, value) in entries {
                if let Some(key) = key {
                    rewrite_current_type_param_bindings_expr(key, bindings, shadowed);
                }
                rewrite_current_type_param_bindings_expr(value, bindings, shadowed);
            }
        }
        Expr::IfExpr {
            body,
            condition,
            else_body,
        } => {
            rewrite_current_type_param_bindings_expr(body, bindings, shadowed);
            rewrite_current_type_param_bindings_expr(condition, bindings, shadowed);
            rewrite_current_type_param_bindings_expr(else_body, bindings, shadowed);
        }
        Expr::Lambda { params, body } => {
            rewrite_current_type_param_bindings_in_params(params, bindings, shadowed);
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
            rewrite_current_type_param_bindings_expr(body, bindings, shadowed);
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
            rewrite_current_type_param_bindings_comprehension(
                element, generators, bindings, shadowed,
            );
        }
        Expr::DictComp {
            key,
            value,
            generators,
        } => {
            rewrite_current_type_param_bindings_comprehension(key, generators, bindings, shadowed);
            rewrite_current_type_param_bindings_expr(value, bindings, shadowed);
        }
        Expr::FString(parts) => {
            for part in parts {
                if let mamba_shared_kernel::ast::FStringPart::Expr(value, format) = part {
                    rewrite_current_type_param_bindings_expr(value, bindings, shadowed);
                    if let Some(format) = format {
                        for part in format {
                            if let mamba_shared_kernel::ast::FStringPart::Expr(value, _) = part {
                                rewrite_current_type_param_bindings_expr(value, bindings, shadowed);
                            }
                        }
                    }
                }
            }
        }
        Expr::Yield(value) => {
            if let Some(value) = value {
                rewrite_current_type_param_bindings_expr(value, bindings, shadowed);
            }
        }
        Expr::Walrus { target, value } => {
            rewrite_current_type_param_bindings_expr(value, bindings, shadowed);
            shadowed.insert(target.clone());
        }
        Expr::ChainedCompare { operands, .. } => {
            for operand in operands {
                rewrite_current_type_param_bindings_expr(operand, bindings, shadowed);
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
