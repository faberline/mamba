use super::*;

pub(super) fn collect_runtime_type_param_uses_in_type_params(
    type_params: &[TypeParam],
    type_param_names: &HashSet<Name>,
    out: &mut HashSet<Name>,
    shadowed: &mut HashSet<Name>,
) {
    for param in type_params {
        let inserted = shadowed.insert(param.name.clone());
        if let Some(bound) = &param.bound {
            collect_runtime_type_param_uses_expr(bound, type_param_names, out, shadowed);
        }
        if let Some(constraints) = &param.constraints {
            for constraint in constraints {
                collect_runtime_type_param_uses_expr(constraint, type_param_names, out, shadowed);
            }
        }
        if let Some(default) = &param.default {
            collect_runtime_type_param_uses_expr(default, type_param_names, out, shadowed);
        }
        if inserted {
            shadowed.remove(&param.name);
        }
    }
}
