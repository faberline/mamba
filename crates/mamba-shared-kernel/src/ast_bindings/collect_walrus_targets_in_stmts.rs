use super::*;

/// Walk every statement body and descend into expression positions to
/// collect walrus targets. Mirrors collect_assignment_targets but only
/// chases walrus, since walrus targets need a *separate* local-define
/// pass (they can appear inside conditions that aren't assignment stmts).
pub fn collect_walrus_targets_in_stmts(stmts: &[Spanned<Stmt>], assigned: &mut Vec<String>) {
    for stmt in stmts {
        match &stmt.node {
            Stmt::Assign { target, value } => {
                collect_walrus_targets(&target.node, assigned);
                collect_walrus_targets(&value.node, assigned);
            }
            Stmt::AugAssign { target, value, .. } => {
                collect_walrus_targets(&target.node, assigned);
                collect_walrus_targets(&value.node, assigned);
            }
            Stmt::VarDecl { value, .. } => collect_walrus_targets(&value.node, assigned),
            Stmt::ExprStmt(e) => collect_walrus_targets(&e.node, assigned),
            Stmt::Return(Some(e)) => collect_walrus_targets(&e.node, assigned),
            Stmt::Del(target) => collect_walrus_targets(&target.node, assigned),
            Stmt::Raise { value, from } => {
                if let Some(value) = value {
                    collect_walrus_targets(&value.node, assigned);
                }
                if let Some(from) = from {
                    collect_walrus_targets(&from.node, assigned);
                }
            }
            Stmt::If {
                condition,
                body,
                elif_clauses,
                else_body,
            } => {
                collect_walrus_targets(&condition.node, assigned);
                collect_walrus_targets_in_stmts(body, assigned);
                for (c, b) in elif_clauses {
                    collect_walrus_targets(&c.node, assigned);
                    collect_walrus_targets_in_stmts(b, assigned);
                }
                if let Some(eb) = else_body {
                    collect_walrus_targets_in_stmts(eb, assigned);
                }
            }
            Stmt::While {
                condition,
                body,
                else_body,
            } => {
                collect_walrus_targets(&condition.node, assigned);
                collect_walrus_targets_in_stmts(body, assigned);
                if let Some(eb) = else_body {
                    collect_walrus_targets_in_stmts(eb, assigned);
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
                collect_walrus_targets(&iter.node, assigned);
                collect_walrus_targets_in_stmts(body, assigned);
                if let Some(eb) = else_body {
                    collect_walrus_targets_in_stmts(eb, assigned);
                }
            }
            Stmt::With { items, body } | Stmt::AsyncWith { items, body } => {
                for item in items {
                    collect_walrus_targets(&item.context.node, assigned);
                }
                collect_walrus_targets_in_stmts(body, assigned);
            }
            Stmt::Try {
                body,
                handlers,
                else_body,
                finally_body,
            } => {
                collect_walrus_targets_in_stmts(body, assigned);
                for h in handlers {
                    if let Some(e) = &h.exc_type {
                        collect_walrus_targets(&e.node, assigned);
                    }
                    collect_walrus_targets_in_stmts(&h.body, assigned);
                }
                if let Some(eb) = else_body {
                    collect_walrus_targets_in_stmts(eb, assigned);
                }
                if let Some(fb) = finally_body {
                    collect_walrus_targets_in_stmts(fb, assigned);
                }
            }
            Stmt::Match { expr, arms } => {
                collect_walrus_targets(&expr.node, assigned);
                for arm in arms {
                    if let Some(guard) = &arm.guard {
                        collect_walrus_targets(&guard.node, assigned);
                    }
                    collect_walrus_targets_in_stmts(&arm.body, assigned);
                }
            }
            Stmt::Assert { test, msg } => {
                collect_walrus_targets(&test.node, assigned);
                if let Some(m) = msg {
                    collect_walrus_targets(&m.node, assigned);
                }
            }
            Stmt::FnDef {
                decorators, params, ..
            }
            | Stmt::AsyncFnDef {
                decorators, params, ..
            } => {
                for decorator in decorators {
                    collect_walrus_targets(&decorator.node, assigned);
                }
                for param in params {
                    if let Some(default) = &param.default {
                        collect_walrus_targets(&default.node, assigned);
                    }
                }
            }
            Stmt::ClassDef {
                decorators,
                bases,
                keyword_args,
                ..
            } => {
                for decorator in decorators {
                    collect_walrus_targets(&decorator.node, assigned);
                }
                for base in bases {
                    collect_walrus_targets(&base.node, assigned);
                }
                for (_, value) in keyword_args {
                    collect_walrus_targets(&value.node, assigned);
                }
            }
            Stmt::EnumDef { variants, .. } => {
                for variant in variants {
                    for field in &variant.fields {
                        if let Some(default) = &field.default {
                            collect_walrus_targets(&default.node, assigned);
                        }
                    }
                }
            }
            Stmt::TypeAlias { value, .. } => collect_walrus_targets(&value.node, assigned),
            // Nested function/class bodies carry their own scope and are not
            // traversed by the arms above.
            _ => {}
        }
    }
}
