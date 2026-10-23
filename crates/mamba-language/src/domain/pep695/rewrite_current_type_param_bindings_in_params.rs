use super::*;

pub(super) fn rewrite_current_type_param_bindings_in_params(
    params: &mut [mamba_shared_kernel::ast::Param],
    bindings: &HashMap<Name, Name>,
    shadowed: &mut HashSet<Name>,
) {
    for param in params {
        if let Some(default) = &mut param.default {
            rewrite_current_type_param_bindings_expr(default, bindings, shadowed);
        }
    }
}
