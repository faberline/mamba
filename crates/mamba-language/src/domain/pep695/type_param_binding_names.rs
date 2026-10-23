use super::*;

pub(super) fn type_param_binding_names(
    type_params: &[TypeParam],
    class_name: Option<&str>,
) -> Vec<Name> {
    type_params
        .iter()
        .map(|param| private_binding_name(&param.name, class_name))
        .collect()
}
