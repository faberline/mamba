use super::*;

pub(super) fn mangle_param(p: &mut Param, class: Option<&str>) {
    mangle_in_place(&mut p.name, class);
    if let Some(d) = &mut p.default {
        mangle_expr(d, class);
    }
}
