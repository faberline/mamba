use super::*;

pub(super) fn collect_runtime_type_param_uses_in_params(
    params: &[mamba_shared_kernel::ast::Param],
    type_param_names: &HashSet<Name>,
    out: &mut HashSet<Name>,
    shadowed: &mut HashSet<Name>,
) {
    for param in params {
        if let Some(default) = &param.default {
            collect_runtime_type_param_uses_expr(default, type_param_names, out, shadowed);
        }
    }
}
