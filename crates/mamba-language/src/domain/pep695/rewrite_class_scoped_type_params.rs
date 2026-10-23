use super::*;

pub(super) fn rewrite_class_scoped_type_params(
    type_params: &mut [TypeParam],
    class_path: &[Name],
    class_locals: &HashSet<Name>,
    class_type_params: &HashMap<Name, usize>,
) {
    let mut shadowed: HashSet<Name> = type_params.iter().map(|param| param.name.clone()).collect();
    for param in type_params {
        if let Some(bound) = &mut param.bound {
            rewrite_class_alias_value(
                bound,
                class_path,
                class_locals,
                class_type_params,
                &mut shadowed,
            );
        }
        if let Some(constraints) = &mut param.constraints {
            for constraint in constraints {
                rewrite_class_alias_value(
                    constraint,
                    class_path,
                    class_locals,
                    class_type_params,
                    &mut shadowed,
                );
            }
        }
        if let Some(default) = &mut param.default {
            rewrite_class_alias_value(
                default,
                class_path,
                class_locals,
                class_type_params,
                &mut shadowed,
            );
        }
    }
}
