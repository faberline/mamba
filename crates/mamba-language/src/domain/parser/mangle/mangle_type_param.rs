use super::*;

pub(super) fn mangle_type_param(param: &mut TypeParam, class: Option<&str>) {
    if let Some(bound) = &mut param.bound {
        mangle_expr(bound, class);
    }
    if let Some(constraints) = &mut param.constraints {
        for constraint in constraints.iter_mut() {
            mangle_expr(constraint, class);
        }
    }
    if let Some(default) = &mut param.default {
        mangle_expr(default, class);
    }
}
