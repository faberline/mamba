use super::*;

pub(super) fn mangle_in_place(name: &mut String, class: Option<&str>) {
    if let Some(m) = mangled(name, class) {
        *name = m;
    }
}
