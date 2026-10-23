use super::*;

pub(super) fn rewrite_class_alias_value(
    expr: &mut Spanned<Expr>,
    class_path: &[Name],
    class_locals: &HashSet<Name>,
    class_type_params: &HashMap<Name, usize>,
    shadowed: &mut HashSet<Name>,
) {
    match &mut expr.node {
        Expr::Ident(name) if class_locals.contains(name) && !shadowed.contains(name) => {
            *expr = class_attr_expr(class_path, name, expr.span);
        }
        Expr::Ident(name) if class_type_params.contains_key(name) && !shadowed.contains(name) => {
            let index = class_type_params[name];
            *expr = class_type_param_expr(class_path, index, expr.span);
        }
        Expr::BinOp { lhs, rhs, .. } => {
            rewrite_class_alias_value(lhs, class_path, class_locals, class_type_params, shadowed);
            rewrite_class_alias_value(rhs, class_path, class_locals, class_type_params, shadowed);
        }
        Expr::UnaryOp { operand, .. }
        | Expr::Starred(operand)
        | Expr::YieldFrom(operand)
        | Expr::Await(operand) => {
            rewrite_class_alias_value(
                operand,
                class_path,
                class_locals,
                class_type_params,
                shadowed,
            );
        }
        Expr::Call { func, args } => {
            rewrite_class_alias_value(func, class_path, class_locals, class_type_params, shadowed);
            for arg in args {
                match arg {
                    CallArg::Positional(value)
                    | CallArg::StarArg(value)
                    | CallArg::DoubleStarArg(value)
                    | CallArg::Keyword { value, .. } => {
                        rewrite_class_alias_value(
                            value,
                            class_path,
                            class_locals,
                            class_type_params,
                            shadowed,
                        );
                    }
                }
            }
        }
        Expr::Attr { object, .. } => {
            rewrite_class_alias_value(
                object,
                class_path,
                class_locals,
                class_type_params,
                shadowed,
            );
        }
        Expr::Index { object, index } => {
            rewrite_class_alias_value(
                object,
                class_path,
                class_locals,
                class_type_params,
                shadowed,
            );
            rewrite_class_alias_value(index, class_path, class_locals, class_type_params, shadowed);
        }
        Expr::Slice { start, stop, step } => {
            for part in [start, stop, step].into_iter().flatten() {
                rewrite_class_alias_value(
                    part,
                    class_path,
                    class_locals,
                    class_type_params,
                    shadowed,
                );
            }
        }
        Expr::ListLit(items)
        | Expr::SetLit(items)
        | Expr::TupleLit(items)
        | Expr::UnpackTarget(items) => {
            for item in items {
                rewrite_class_alias_value(
                    item,
                    class_path,
                    class_locals,
                    class_type_params,
                    shadowed,
                );
            }
        }
        Expr::DictLit(entries) => {
            for (key, value) in entries {
                if let Some(key) = key {
                    rewrite_class_alias_value(
                        key,
                        class_path,
                        class_locals,
                        class_type_params,
                        shadowed,
                    );
                }
                rewrite_class_alias_value(
                    value,
                    class_path,
                    class_locals,
                    class_type_params,
                    shadowed,
                );
            }
        }
        Expr::IfExpr {
            body,
            condition,
            else_body,
        } => {
            rewrite_class_alias_value(body, class_path, class_locals, class_type_params, shadowed);
            rewrite_class_alias_value(
                condition,
                class_path,
                class_locals,
                class_type_params,
                shadowed,
            );
            rewrite_class_alias_value(
                else_body,
                class_path,
                class_locals,
                class_type_params,
                shadowed,
            );
        }
        Expr::Lambda { params, body } => {
            for param in params.iter_mut() {
                if let Some(default) = &mut param.default {
                    rewrite_class_alias_value(
                        default,
                        class_path,
                        class_locals,
                        class_type_params,
                        shadowed,
                    );
                }
            }
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
            rewrite_class_alias_value(body, class_path, class_locals, class_type_params, shadowed);
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
            rewrite_comprehension(
                element,
                generators,
                class_path,
                class_locals,
                class_type_params,
                shadowed,
            );
        }
        Expr::DictComp {
            key,
            value,
            generators,
        } => {
            rewrite_comprehension(
                key,
                generators,
                class_path,
                class_locals,
                class_type_params,
                shadowed,
            );
            rewrite_class_alias_value(value, class_path, class_locals, class_type_params, shadowed);
        }
        Expr::FString(parts) => {
            for part in parts {
                if let mamba_shared_kernel::ast::FStringPart::Expr(value, format) = part {
                    rewrite_class_alias_value(
                        value,
                        class_path,
                        class_locals,
                        class_type_params,
                        shadowed,
                    );
                    if let Some(format) = format {
                        for part in format {
                            if let mamba_shared_kernel::ast::FStringPart::Expr(value, _) = part {
                                rewrite_class_alias_value(
                                    value,
                                    class_path,
                                    class_locals,
                                    class_type_params,
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
                rewrite_class_alias_value(
                    value,
                    class_path,
                    class_locals,
                    class_type_params,
                    shadowed,
                );
            }
        }
        Expr::Walrus { target, value } => {
            rewrite_class_alias_value(value, class_path, class_locals, class_type_params, shadowed);
            shadowed.insert(target.clone());
        }
        Expr::ChainedCompare { operands, .. } => {
            for operand in operands {
                rewrite_class_alias_value(
                    operand,
                    class_path,
                    class_locals,
                    class_type_params,
                    shadowed,
                );
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
