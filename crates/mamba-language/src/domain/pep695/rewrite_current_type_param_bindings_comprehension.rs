use super::*;

pub(super) fn rewrite_current_type_param_bindings_comprehension(
    element: &mut Spanned<Expr>,
    generators: &mut [mamba_shared_kernel::ast::Comprehension],
    bindings: &HashMap<Name, Name>,
    shadowed: &mut HashSet<Name>,
) {
    let mut added = Vec::new();
    for generator in generators {
        rewrite_current_type_param_bindings_expr(&mut generator.iter, bindings, shadowed);
        for target in &generator.targets {
            if shadowed.insert(target.clone()) {
                added.push(target.clone());
            }
        }
        for condition in &mut generator.conditions {
            rewrite_current_type_param_bindings_expr(condition, bindings, shadowed);
        }
    }
    rewrite_current_type_param_bindings_expr(element, bindings, shadowed);
    for name in added {
        shadowed.remove(&name);
    }
}
