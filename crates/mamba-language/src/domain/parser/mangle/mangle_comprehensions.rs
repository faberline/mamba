use super::*;

pub(super) fn mangle_comprehensions(generators: &mut [Comprehension], class: Option<&str>) {
    for gen in generators.iter_mut() {
        for t in gen.targets.iter_mut() {
            mangle_in_place(t, class);
        }
        mangle_expr(&mut gen.iter, class);
        for c in gen.conditions.iter_mut() {
            mangle_expr(c, class);
        }
    }
}
