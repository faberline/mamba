use super::*;

pub(super) fn collect_class_local_names(stmts: &[Spanned<Stmt>]) -> HashSet<Name> {
    let mut names = HashSet::new();
    for stmt in stmts {
        match &stmt.node {
            Stmt::VarDecl { name, .. }
            | Stmt::BareAnnotation { name, .. }
            | Stmt::FnDef { name, .. }
            | Stmt::AsyncFnDef { name, .. }
            | Stmt::ClassDef { name, .. }
            | Stmt::EnumDef { name, .. }
            | Stmt::TypeAlias { name, .. } => {
                names.insert(name.clone());
            }
            Stmt::Assign { target, .. } | Stmt::AugAssign { target, .. } => {
                collect_target_names(&target.node, &mut names);
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
                names.extend(targets.iter().cloned());
                names.extend(collect_class_local_names(body));
                if let Some(body) = else_body {
                    names.extend(collect_class_local_names(body));
                }
            }
            Stmt::Import {
                module,
                names: Some(imports),
                ..
            } => {
                let _ = module;
                for (name, alias) in imports {
                    names.insert(alias.as_ref().unwrap_or(name).clone());
                }
            }
            Stmt::Import {
                module,
                names: None,
                module_alias,
            } => {
                if let Some(alias) = module_alias {
                    names.insert(alias.clone());
                } else if let Some(root) = module.first() {
                    names.insert(root.clone());
                }
            }
            Stmt::With { items, body } | Stmt::AsyncWith { items, body } => {
                for item in items {
                    if let Some(alias) = &item.alias {
                        names.insert(alias.clone());
                    }
                }
                names.extend(collect_class_local_names(body));
            }
            Stmt::If {
                body,
                elif_clauses,
                else_body,
                ..
            } => {
                names.extend(collect_class_local_names(body));
                for (_, body) in elif_clauses {
                    names.extend(collect_class_local_names(body));
                }
                if let Some(body) = else_body {
                    names.extend(collect_class_local_names(body));
                }
            }
            Stmt::While {
                body, else_body, ..
            } => {
                names.extend(collect_class_local_names(body));
                if let Some(body) = else_body {
                    names.extend(collect_class_local_names(body));
                }
            }
            Stmt::Try {
                body,
                handlers,
                else_body,
                finally_body,
            } => {
                names.extend(collect_class_local_names(body));
                for handler in handlers {
                    if let Some(name) = &handler.name {
                        names.insert(name.clone());
                    }
                    names.extend(collect_class_local_names(&handler.body));
                }
                if let Some(body) = else_body {
                    names.extend(collect_class_local_names(body));
                }
                if let Some(body) = finally_body {
                    names.extend(collect_class_local_names(body));
                }
            }
            Stmt::Match { arms, .. } => {
                for arm in arms {
                    names.extend(collect_class_local_names(&arm.body));
                }
            }
            _ => {}
        }
    }
    names
}
