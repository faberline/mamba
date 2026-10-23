use super::*;

pub(super) fn rewrite_current_type_param_bindings_in_type_params(
    type_params: &mut [TypeParam],
    bindings: &HashMap<Name, Name>,
    shadowed: &mut HashSet<Name>,
) {
    for param in type_params {
        let inserted = shadowed.insert(param.name.clone());
        if let Some(bound) = &mut param.bound {
            rewrite_current_type_param_bindings_expr(bound, bindings, shadowed);
        }
        if let Some(constraints) = &mut param.constraints {
            for constraint in constraints {
                rewrite_current_type_param_bindings_expr(constraint, bindings, shadowed);
            }
        }
        if let Some(default) = &mut param.default {
            rewrite_current_type_param_bindings_expr(default, bindings, shadowed);
        }
        if inserted {
            shadowed.remove(&param.name);
        }
    }
}
