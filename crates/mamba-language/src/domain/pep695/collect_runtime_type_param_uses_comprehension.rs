use super::*;

pub(super) fn collect_runtime_type_param_uses_comprehension(
    element: &Spanned<Expr>,
    generators: &[mamba_shared_kernel::ast::Comprehension],
    type_param_names: &HashSet<Name>,
    out: &mut HashSet<Name>,
    shadowed: &mut HashSet<Name>,
) {
    let mut added = Vec::new();
    for generator in generators {
        collect_runtime_type_param_uses_expr(&generator.iter, type_param_names, out, shadowed);
        for target in &generator.targets {
            if shadowed.insert(target.clone()) {
                added.push(target.clone());
            }
        }
        for condition in &generator.conditions {
            collect_runtime_type_param_uses_expr(condition, type_param_names, out, shadowed);
        }
    }
    collect_runtime_type_param_uses_expr(element, type_param_names, out, shadowed);
    for name in added {
        shadowed.remove(&name);
    }
}
