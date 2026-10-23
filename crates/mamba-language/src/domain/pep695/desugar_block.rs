use super::*;

/// Desugar one statement block. `in_class` marks class bodies, whose typevar
/// bindings must hoist out to the nearest non-class scope. Returns the
/// hoists destined for the enclosing block (non-empty only for class bodies).
pub(super) fn desugar_block(
    stmts: &mut Vec<Spanned<Stmt>>,
    in_class: bool,
    class_path: &[Name],
    class_locals: Option<&HashSet<Name>>,
    class_type_params: Option<&HashMap<Name, usize>>,
) -> ClassHoists {
    let old = std::mem::take(stmts);
    let mut out: Vec<Spanned<Stmt>> = Vec::new();
    let mut hoist_up = ClassHoists {
        before: Vec::new(),
        after: Vec::new(),
    };
    let mut visible_class_locals = class_locals.cloned().unwrap_or_default();

    // Emit one deferred wiring item into a non-class block.

    for mut st in old {
        let stmt_class_locals =
            in_class.then(|| collect_class_local_names(std::slice::from_ref(&st)));
        let span = st.span;
        match &mut st.node {
            // ── class definitions (recursion + emission together) ──
            Stmt::ClassDef { .. } => {
                class_desugar::desugar_class(st, in_class, class_path, &mut out, &mut hoist_up);
            }
            // ── function definitions ──
            Stmt::FnDef {
                name,
                type_params,
                body,
                ..
            }
            | Stmt::AsyncFnDef {
                name,
                type_params,
                body,
                ..
            } => {
                if let (true, Some(type_params_by_name)) = (in_class, class_type_params) {
                    if !class_path.is_empty() {
                        rewrite_class_scoped_type_params(
                            type_params,
                            class_path,
                            &visible_class_locals,
                            type_params_by_name,
                        );
                    }
                }
                let name = name.clone();
                let tps = type_params.clone();
                let binding_names =
                    type_param_binding_names(&tps, class_path.last().map(String::as_str));
                let extra_attrs = class_type_params
                    .filter(|type_params_by_name| !type_params_by_name.is_empty())
                    .filter(|_| !class_path.is_empty())
                    .map(|_| {
                        vec![(
                            "__mb_class_type_params__".to_string(),
                            class_attr_expr(class_path, "__type_params__", span),
                        )]
                    })
                    .unwrap_or_default();
                let _ = desugar_block(body, false, &[], None, None);
                if tps.is_empty() {
                    out.push(st);
                    if in_class {
                        hoist_up.after.push(AfterItem {
                            tv_assigns: Vec::new(),
                            path: vec![name],
                            attrs: vec![("__type_params__".to_string(), Vec::new())],
                            extra_attrs,
                            span,
                        });
                    } else {
                        out.push(attr_tuple_assign(&[name], "__type_params__", &[], span));
                    }
                } else {
                    let tv_assigns: Vec<Spanned<Stmt>> = tps
                        .iter()
                        .zip(binding_names.iter())
                        .map(|(param, binding_name)| typevar_assign(param, binding_name, span))
                        .collect();
                    if in_class {
                        // Defer the whole wiring to after the enclosing class
                        // so a same-named class param's already-captured tuple
                        // stays intact (see ClassHoists docs).
                        out.push(st);
                        hoist_up.after.push(AfterItem {
                            tv_assigns,
                            path: vec![name],
                            attrs: vec![("__type_params__".to_string(), binding_names)],
                            extra_attrs,
                            span,
                        });
                    } else {
                        // Function defaults/decorators execute as part of the
                        // `def` statement, before the PEP 695 annotation scope
                        // becomes visible at runtime. Emit the runtime TypeVar
                        // bindings after the `def` so reads like
                        // `def f[T](x=list[T]()): ...` still raise NameError,
                        // while the body can resolve `T` later through the
                        // enclosing scope.
                        out.push(st);
                        out.extend(tv_assigns);
                        out.push(attr_tuple_assign(
                            &[name],
                            "__type_params__",
                            &binding_names,
                            span,
                        ));
                    }
                }
            }
            // ── type aliases ──
            Stmt::TypeAlias {
                name,
                type_params,
                value,
            } => {
                let name = name.clone();
                let tps = type_params.clone();
                let binding_names =
                    type_param_binding_names(&tps, class_path.last().map(String::as_str));
                let mut value = value.clone();
                if let (true, Some(type_params)) = (in_class, class_type_params) {
                    if !class_path.is_empty() {
                        let mut alias_locals = visible_class_locals.clone();
                        alias_locals.insert(name.clone());
                        rewrite_class_alias_value(
                            &mut value,
                            class_path,
                            &alias_locals,
                            type_params,
                            &mut HashSet::new(),
                        );
                    }
                }
                // Param typevars must exist when the alias's constructor call
                // runs (it captures the params tuple eagerly), so they go
                // before the (outermost) class statement when in a class body.
                let tv_assigns: Vec<Spanned<Stmt>> = tps
                    .iter()
                    .zip(binding_names.iter())
                    .map(|(param, binding_name)| typevar_assign(param, binding_name, span))
                    .collect();
                if in_class {
                    hoist_up.before.extend(tv_assigns);
                } else {
                    out.extend(tv_assigns);
                }
                // Keep the original statement: the type checker registers the
                // compile-time alias from it; HIR lowering skips it.
                out.push(st);
                // Placeholder binding (an empty TypeAliasType, type `any`) so
                // the lazy value thunk below may reference the alias's own
                // name — recursive aliases (`type R = R | None`) resolve.
                let placeholder = Expr::Call {
                    func: Box::new(sp(Expr::Ident(TYPE_ALIAS_INTRINSIC.to_string()), span)),
                    args: vec![
                        CallArg::Positional(sp(Expr::StrLit(name.clone()), span)),
                        CallArg::Positional(sp(Expr::NoneLit, span)),
                        CallArg::Positional(sp(Expr::TupleLit(vec![]), span)),
                    ],
                };
                out.push(Spanned::new(
                    Stmt::Assign {
                        target: sp(Expr::Ident(name.clone()), span),
                        value: sp(placeholder, span),
                    },
                    span,
                ));
                // X = __mb_pep695_type_alias__("X", lambda: V, (params...))
                let thunk = sp(
                    Expr::Lambda {
                        params: vec![],
                        body: Box::new(value),
                    },
                    span,
                );
                let call = Expr::Call {
                    func: Box::new(sp(Expr::Ident(TYPE_ALIAS_INTRINSIC.to_string()), span)),
                    args: vec![
                        CallArg::Positional(sp(Expr::StrLit(name.clone()), span)),
                        CallArg::Positional(thunk),
                        CallArg::Positional(params_tuple(&binding_names, span)),
                    ],
                };
                out.push(Spanned::new(
                    Stmt::Assign {
                        target: sp(Expr::Ident(name), span),
                        value: sp(call, span),
                    },
                    span,
                ));
            }
            // ── same-scope compound statements ──
            _ => {
                // Route hoists from a same-scope sub-block (if/try/...):
                // for non-class blocks emit immediately (paths complete);
                // for class blocks keep bubbling.
                let route =
                    |h: ClassHoists, out: &mut Vec<Spanned<Stmt>>, hoist_up: &mut ClassHoists| {
                        if in_class {
                            hoist_up.before.extend(h.before);
                            hoist_up.after.extend(h.after);
                        } else {
                            out.extend(h.before);
                            for item in h.after {
                                emit_after(out, item);
                            }
                        }
                    };
                match &mut st.node {
                    Stmt::If {
                        body,
                        elif_clauses,
                        else_body,
                        ..
                    } => {
                        let h = desugar_block(
                            body,
                            in_class,
                            class_path,
                            class_locals,
                            class_type_params,
                        );
                        route(h, &mut out, &mut hoist_up);
                        for (_, b) in elif_clauses {
                            let h = desugar_block(
                                b,
                                in_class,
                                class_path,
                                class_locals,
                                class_type_params,
                            );
                            route(h, &mut out, &mut hoist_up);
                        }
                        if let Some(b) = else_body {
                            let h = desugar_block(
                                b,
                                in_class,
                                class_path,
                                class_locals,
                                class_type_params,
                            );
                            route(h, &mut out, &mut hoist_up);
                        }
                    }
                    Stmt::While {
                        body, else_body, ..
                    }
                    | Stmt::For {
                        body, else_body, ..
                    }
                    | Stmt::AsyncFor {
                        body, else_body, ..
                    } => {
                        let h = desugar_block(
                            body,
                            in_class,
                            class_path,
                            class_locals,
                            class_type_params,
                        );
                        route(h, &mut out, &mut hoist_up);
                        if let Some(b) = else_body {
                            let h = desugar_block(
                                b,
                                in_class,
                                class_path,
                                class_locals,
                                class_type_params,
                            );
                            route(h, &mut out, &mut hoist_up);
                        }
                    }
                    Stmt::With { body, .. } | Stmt::AsyncWith { body, .. } => {
                        let h = desugar_block(
                            body,
                            in_class,
                            class_path,
                            class_locals,
                            class_type_params,
                        );
                        route(h, &mut out, &mut hoist_up);
                    }
                    Stmt::Try {
                        body,
                        handlers,
                        else_body,
                        finally_body,
                    } => {
                        let h = desugar_block(
                            body,
                            in_class,
                            class_path,
                            class_locals,
                            class_type_params,
                        );
                        route(h, &mut out, &mut hoist_up);
                        for handler in handlers {
                            let h = desugar_block(
                                &mut handler.body,
                                in_class,
                                class_path,
                                class_locals,
                                class_type_params,
                            );
                            route(h, &mut out, &mut hoist_up);
                        }
                        if let Some(b) = else_body {
                            let h = desugar_block(
                                b,
                                in_class,
                                class_path,
                                class_locals,
                                class_type_params,
                            );
                            route(h, &mut out, &mut hoist_up);
                        }
                        if let Some(b) = finally_body {
                            let h = desugar_block(
                                b,
                                in_class,
                                class_path,
                                class_locals,
                                class_type_params,
                            );
                            route(h, &mut out, &mut hoist_up);
                        }
                    }
                    Stmt::Match { arms, .. } => {
                        for arm in arms {
                            let h = desugar_block(
                                &mut arm.body,
                                in_class,
                                class_path,
                                class_locals,
                                class_type_params,
                            );
                            route(h, &mut out, &mut hoist_up);
                        }
                    }
                    _ => {}
                }
                out.push(st);
            }
        }

        if let Some(stmt_class_locals) = stmt_class_locals {
            visible_class_locals.extend(stmt_class_locals);
        }
    }

    *stmts = out;
    hoist_up
}
