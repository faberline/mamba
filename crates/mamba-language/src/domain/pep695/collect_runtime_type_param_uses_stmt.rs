use super::*;

pub(super) fn collect_runtime_type_param_uses_stmt(
    stmt: &Spanned<Stmt>,
    type_param_names: &HashSet<Name>,
    out: &mut HashSet<Name>,
    shadowed: &mut HashSet<Name>,
) {
    match &stmt.node {
        Stmt::VarDecl { value, .. } => {
            collect_runtime_type_param_uses_expr(value, type_param_names, out, shadowed);
        }
        Stmt::Assign { target, value } | Stmt::AugAssign { target, value, .. } => {
            collect_runtime_type_param_uses_expr(target, type_param_names, out, shadowed);
            collect_runtime_type_param_uses_expr(value, type_param_names, out, shadowed);
        }
        Stmt::FnDef {
            decorators,
            type_params,
            params,
            body,
            ..
        }
        | Stmt::AsyncFnDef {
            decorators,
            type_params,
            params,
            body,
            ..
        } => {
            for decorator in decorators {
                collect_runtime_type_param_uses_expr(decorator, type_param_names, out, shadowed);
            }
            collect_runtime_type_param_uses_in_params(params, type_param_names, out, shadowed);
            let added: Vec<Name> = params
                .iter()
                .map(|param| param.name.clone())
                .chain(type_params.iter().map(|param| param.name.clone()))
                .filter_map(|name| {
                    if shadowed.insert(name.clone()) {
                        Some(name)
                    } else {
                        None
                    }
                })
                .collect();
            collect_runtime_type_param_uses_block(body, type_param_names, out, shadowed);
            for name in added {
                shadowed.remove(&name);
            }
        }
        Stmt::ClassDef {
            decorators,
            type_params,
            bases,
            keyword_args,
            body,
            ..
        } => {
            for decorator in decorators {
                collect_runtime_type_param_uses_expr(decorator, type_param_names, out, shadowed);
            }
            collect_runtime_type_param_uses_in_type_params(
                type_params,
                type_param_names,
                out,
                shadowed,
            );
            for base in bases {
                collect_runtime_type_param_uses_expr(base, type_param_names, out, shadowed);
            }
            for (_, value) in keyword_args {
                collect_runtime_type_param_uses_expr(value, type_param_names, out, shadowed);
            }
            let added: Vec<Name> = type_params
                .iter()
                .filter_map(|param| {
                    if shadowed.insert(param.name.clone()) {
                        Some(param.name.clone())
                    } else {
                        None
                    }
                })
                .collect();
            collect_runtime_type_param_uses_block(body, type_param_names, out, shadowed);
            for name in added {
                shadowed.remove(&name);
            }
        }
        Stmt::If {
            condition,
            body,
            elif_clauses,
            else_body,
        } => {
            collect_runtime_type_param_uses_expr(condition, type_param_names, out, shadowed);
            collect_runtime_type_param_uses_block(body, type_param_names, out, shadowed);
            for (condition, body) in elif_clauses {
                collect_runtime_type_param_uses_expr(condition, type_param_names, out, shadowed);
                collect_runtime_type_param_uses_block(body, type_param_names, out, shadowed);
            }
            if let Some(body) = else_body {
                collect_runtime_type_param_uses_block(body, type_param_names, out, shadowed);
            }
        }
        Stmt::While {
            condition,
            body,
            else_body,
        } => {
            collect_runtime_type_param_uses_expr(condition, type_param_names, out, shadowed);
            collect_runtime_type_param_uses_block(body, type_param_names, out, shadowed);
            if let Some(body) = else_body {
                collect_runtime_type_param_uses_block(body, type_param_names, out, shadowed);
            }
        }
        Stmt::For {
            iter,
            body,
            else_body,
            ..
        }
        | Stmt::AsyncFor {
            iter,
            body,
            else_body,
            ..
        } => {
            collect_runtime_type_param_uses_expr(iter, type_param_names, out, shadowed);
            collect_runtime_type_param_uses_block(body, type_param_names, out, shadowed);
            if let Some(body) = else_body {
                collect_runtime_type_param_uses_block(body, type_param_names, out, shadowed);
            }
        }
        Stmt::Match { expr, arms } => {
            collect_runtime_type_param_uses_expr(expr, type_param_names, out, shadowed);
            for arm in arms {
                if let Some(guard) = &arm.guard {
                    collect_runtime_type_param_uses_expr(guard, type_param_names, out, shadowed);
                }
                collect_runtime_type_param_uses_block(&arm.body, type_param_names, out, shadowed);
            }
        }
        Stmt::Return(value) => {
            if let Some(value) = value {
                collect_runtime_type_param_uses_expr(value, type_param_names, out, shadowed);
            }
        }
        Stmt::Try {
            body,
            handlers,
            else_body,
            finally_body,
        } => {
            collect_runtime_type_param_uses_block(body, type_param_names, out, shadowed);
            for handler in handlers {
                if let Some(exc_type) = &handler.exc_type {
                    collect_runtime_type_param_uses_expr(exc_type, type_param_names, out, shadowed);
                }
                collect_runtime_type_param_uses_block(
                    &handler.body,
                    type_param_names,
                    out,
                    shadowed,
                );
            }
            if let Some(body) = else_body {
                collect_runtime_type_param_uses_block(body, type_param_names, out, shadowed);
            }
            if let Some(body) = finally_body {
                collect_runtime_type_param_uses_block(body, type_param_names, out, shadowed);
            }
        }
        Stmt::Raise { value, from } => {
            if let Some(value) = value {
                collect_runtime_type_param_uses_expr(value, type_param_names, out, shadowed);
            }
            if let Some(from) = from {
                collect_runtime_type_param_uses_expr(from, type_param_names, out, shadowed);
            }
        }
        Stmt::With { items, body } | Stmt::AsyncWith { items, body } => {
            for item in items {
                collect_runtime_type_param_uses_expr(
                    &item.context,
                    type_param_names,
                    out,
                    shadowed,
                );
            }
            collect_runtime_type_param_uses_block(body, type_param_names, out, shadowed);
        }
        Stmt::Assert { test, msg } => {
            collect_runtime_type_param_uses_expr(test, type_param_names, out, shadowed);
            if let Some(msg) = msg {
                collect_runtime_type_param_uses_expr(msg, type_param_names, out, shadowed);
            }
        }
        Stmt::Del(target) | Stmt::ExprStmt(target) => {
            collect_runtime_type_param_uses_expr(target, type_param_names, out, shadowed);
        }
        Stmt::TypeAlias { .. }
        | Stmt::EnumDef { .. }
        | Stmt::BareAnnotation { .. }
        | Stmt::Import { .. }
        | Stmt::Pass
        | Stmt::Break
        | Stmt::Continue
        | Stmt::Global(_)
        | Stmt::Nonlocal(_) => {}
    }
}
