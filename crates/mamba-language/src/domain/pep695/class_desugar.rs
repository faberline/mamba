//! Class annotation-scope bindings and deferred attribute wiring.
use super::*;

pub(super) fn desugar_class(
    mut st: Spanned<Stmt>,
    in_class: bool,
    class_path: &[Name],
    out: &mut Vec<Spanned<Stmt>>,
    hoist_up: &mut ClassHoists,
) {
    let span = st.span;
    let Stmt::ClassDef {
        name,
        type_params,
        decorators,
        bases,
        keyword_args,
        body,
        ..
    } = &mut st.node
    else {
        unreachable!("class desugaring requires a class statement");
    };

    let name = name.clone();
    let binding_names = type_param_binding_names(type_params, Some(name.as_str()));
    let binding_map: HashMap<Name, Name> = type_params
        .iter()
        .zip(binding_names.iter())
        .filter(|(param, binding_name)| &param.name != *binding_name)
        .map(|(param, binding_name)| (param.name.clone(), binding_name.clone()))
        .collect();
    if !binding_map.is_empty() {
        rewrite_current_type_param_bindings_in_type_params(
            type_params,
            &binding_map,
            &mut HashSet::new(),
        );
        for decorator in decorators.iter_mut() {
            rewrite_current_type_param_bindings_expr(decorator, &binding_map, &mut HashSet::new());
        }
        for base in bases.iter_mut() {
            rewrite_current_type_param_bindings_expr(base, &binding_map, &mut HashSet::new());
        }
        for (_, value) in keyword_args.iter_mut() {
            rewrite_current_type_param_bindings_expr(value, &binding_map, &mut HashSet::new());
        }
    }
    let tps = type_params.clone();
    let type_param_names: HashSet<Name> = binding_names.iter().cloned().collect();
    let mut runtime_type_param_uses = HashSet::new();
    for decorator in decorators.iter() {
        collect_runtime_type_param_uses_expr(
            decorator,
            &type_param_names,
            &mut runtime_type_param_uses,
            &mut HashSet::new(),
        );
    }
    collect_runtime_type_param_uses_in_type_params(
        type_params,
        &type_param_names,
        &mut runtime_type_param_uses,
        &mut HashSet::new(),
    );
    for base in bases.iter() {
        collect_runtime_type_param_uses_expr(
            base,
            &type_param_names,
            &mut runtime_type_param_uses,
            &mut HashSet::new(),
        );
    }
    for (_, value) in keyword_args.iter() {
        collect_runtime_type_param_uses_expr(
            value,
            &type_param_names,
            &mut runtime_type_param_uses,
            &mut HashSet::new(),
        );
    }
    collect_runtime_type_param_uses_block(
        body,
        &type_param_names,
        &mut runtime_type_param_uses,
        &mut HashSet::new(),
    );
    let mut nested_path = class_path.to_vec();
    nested_path.push(name.clone());
    let nested_type_params: HashMap<Name, usize> = binding_names
        .iter()
        .enumerate()
        .map(|(index, binding_name)| (binding_name.clone(), index))
        .collect();
    let mut h = desugar_block(body, true, &nested_path, None, Some(&nested_type_params));
    // Items leaving this class body gain its name as path prefix.
    for item in &mut h.after {
        item.path.insert(0, name.clone());
    }
    let mut tv_assigns: Vec<Spanned<Stmt>> = Vec::new();
    for (param, binding_name) in tps.iter().zip(binding_names.iter()) {
        tv_assigns.push(typevar_assign(param, binding_name, span));
        if binding_name != &param.name {
            tv_assigns.push(alias_ident(&param.name, binding_name, span));
        }
    }
    let attrs = if tps.is_empty() {
        Vec::new()
    } else {
        vec![
            ("__type_params__".to_string(), binding_names.clone()),
            // Generic classes also expose a matching __parameters__.
            ("__parameters__".to_string(), binding_names.clone()),
        ]
    };
    if in_class {
        // Nested class: its body executes while the outer class
        // body runs, so everything needed *before* keeps
        // bubbling; the attribute wiring keeps deferring.
        hoist_up.before.extend(h.before);
        hoist_up.before.extend(tv_assigns);
        out.push(st);
        if !attrs.is_empty() {
            hoist_up.after.push(AfterItem {
                tv_assigns: Vec::new(),
                path: vec![name],
                attrs,
                extra_attrs: Vec::new(),
                span,
            });
        }
        hoist_up.after.extend(h.after);
    } else {
        let mut alias_saved_bindings: Vec<SavedBinding> = Vec::new();
        let mut temp_saved_bindings: Vec<SavedBinding> = Vec::new();
        let mut binding_slot = 0usize;
        for (param, binding_name) in tps.iter().zip(binding_names.iter()) {
            if binding_name != &param.name {
                alias_saved_bindings.push(saved_binding(&param.name, binding_slot, span));
                binding_slot += 1;
            }
        }
        for binding_name in &binding_names {
            if !runtime_type_param_uses.contains(binding_name) {
                temp_saved_bindings.push(saved_binding(binding_name, binding_slot, span));
                binding_slot += 1;
            }
        }
        out.extend(h.before);
        for binding in &alias_saved_bindings {
            out.push(save_binding(binding, span));
        }
        for binding in &temp_saved_bindings {
            out.push(save_binding(binding, span));
        }
        out.extend(tv_assigns);
        out.push(st);
        for (attr, binding_names) in &attrs {
            out.push(attr_tuple_assign(
                std::slice::from_ref(&name),
                attr,
                binding_names,
                span,
            ));
        }
        for binding in &alias_saved_bindings {
            out.extend(restore_or_delete_binding(binding, span));
        }
        for binding in &temp_saved_bindings {
            out.extend(restore_or_delete_binding(binding, span));
        }
        for item in h.after {
            emit_after(out, item);
        }
    }
}
