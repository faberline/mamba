use super::*;

/// Pre-scan a function body for names that appear as assignment targets, so
/// the resolver can define them as local BEFORE walking the statements. Needed
/// because Python scopes on assignment (any name assigned in a function body
/// is local unless declared `global` / `nonlocal`), while a naive lookup()
/// would find an outer-scope symbol and rebind that instead.
///
/// Does NOT descend into nested FnDef / AsyncFnDef / ClassDef bodies — those
/// carry their own scope. `declared` collects names shadowed by `global` /
/// `nonlocal` so the caller can skip pre-defining them.
pub fn collect_assignment_targets(
    stmts: &[Spanned<Stmt>],
    assigned: &mut Vec<String>,
    declared: &mut Vec<String>,
) {
    for stmt in stmts {
        match &stmt.node {
            Stmt::VarDecl { name, .. } => {
                if !assigned.iter().any(|n| n == name) {
                    assigned.push(name.clone());
                }
            }
            Stmt::BareAnnotation { name, .. } => {
                if !assigned.iter().any(|n| n == name) {
                    assigned.push(name.clone());
                }
            }
            Stmt::TypeAlias { name, .. } => {
                if !assigned.iter().any(|n| n == name) {
                    assigned.push(name.clone());
                }
            }
            Stmt::Assign { target, .. } | Stmt::AugAssign { target, .. } | Stmt::Del(target) => {
                collect_expr_targets(&target.node, assigned);
            }
            Stmt::For {
                targets,
                body,
                else_body,
                ..
            }
            | Stmt::AsyncFor {
                targets,
                body,
                else_body,
                ..
            } => {
                for t in targets {
                    if !assigned.iter().any(|n| n == t) {
                        assigned.push(t.clone());
                    }
                }
                collect_assignment_targets(body, assigned, declared);
                if let Some(eb) = else_body {
                    collect_assignment_targets(eb, assigned, declared);
                }
            }
            Stmt::With { items, body } | Stmt::AsyncWith { items, body } => {
                for item in items {
                    if let Some(name) = &item.alias {
                        if !assigned.iter().any(|n| n == name) {
                            assigned.push(name.clone());
                        }
                    }
                }
                collect_assignment_targets(body, assigned, declared);
            }
            Stmt::Try {
                body,
                handlers,
                else_body,
                finally_body,
            } => {
                collect_assignment_targets(body, assigned, declared);
                for h in handlers {
                    if let Some(name) = &h.name {
                        if !assigned.iter().any(|n| n == name) {
                            assigned.push(name.clone());
                        }
                    }
                    collect_assignment_targets(&h.body, assigned, declared);
                }
                if let Some(eb) = else_body {
                    collect_assignment_targets(eb, assigned, declared);
                }
                if let Some(fb) = finally_body {
                    collect_assignment_targets(fb, assigned, declared);
                }
            }
            Stmt::If {
                body,
                elif_clauses,
                else_body,
                ..
            } => {
                collect_assignment_targets(body, assigned, declared);
                for (_, b) in elif_clauses {
                    collect_assignment_targets(b, assigned, declared);
                }
                if let Some(eb) = else_body {
                    collect_assignment_targets(eb, assigned, declared);
                }
            }
            Stmt::While {
                body, else_body, ..
            } => {
                collect_assignment_targets(body, assigned, declared);
                if let Some(eb) = else_body {
                    collect_assignment_targets(eb, assigned, declared);
                }
            }
            Stmt::Match { arms, .. } => {
                for arm in arms {
                    collect_pattern_targets(&arm.pattern.node, assigned);
                    collect_assignment_targets(&arm.body, assigned, declared);
                }
            }
            Stmt::Global(names) | Stmt::Nonlocal(names) => {
                for n in names {
                    declared.push(n.clone());
                }
            }
            Stmt::Import {
                names: Some(names),
                module,
                module_alias,
            } => {
                for (n, alias) in names {
                    let binding = alias.clone().unwrap_or_else(|| n.clone());
                    if !assigned.iter().any(|x| x == &binding) {
                        assigned.push(binding);
                    }
                }
                // `import a.b as c` binds `c`; `import a` binds `a`.
                if names.is_empty() {
                    if let Some(alias) = module_alias {
                        if !assigned.iter().any(|x| x == alias) {
                            assigned.push(alias.clone());
                        }
                    } else if let Some(first) = module.first() {
                        if !assigned.iter().any(|x| x == first) {
                            assigned.push(first.clone());
                        }
                    }
                }
            }
            Stmt::Import {
                module,
                module_alias,
                names: None,
            } => {
                let binding = module_alias.clone().or_else(|| module.first().cloned());
                if let Some(b) = binding {
                    if !assigned.iter().any(|x| x == &b) {
                        assigned.push(b);
                    }
                }
            }
            // Nested function / class definitions bind the name but do NOT
            // contribute their body's assignments to the outer scope.
            Stmt::FnDef { name, .. }
            | Stmt::AsyncFnDef { name, .. }
            | Stmt::ClassDef { name, .. }
            | Stmt::EnumDef { name, .. } => {
                if !assigned.iter().any(|n| n == name) {
                    assigned.push(name.clone());
                }
            }
            _ => {}
        }
    }
}
