use super::*;

pub(super) fn rewrite_comprehension(
    element: &mut Spanned<Expr>,
    generators: &mut [mamba_shared_kernel::ast::Comprehension],
    class_path: &[Name],
    class_locals: &HashSet<Name>,
    class_type_params: &HashMap<Name, usize>,
    shadowed: &mut HashSet<Name>,
) {
    let mut added = Vec::new();
    for generator in generators {
        rewrite_class_alias_value(
            &mut generator.iter,
            class_path,
            class_locals,
            class_type_params,
            shadowed,
        );
        for target in &generator.targets {
            if shadowed.insert(target.clone()) {
                added.push(target.clone());
            }
        }
        for condition in &mut generator.conditions {
            rewrite_class_alias_value(
                condition,
                class_path,
                class_locals,
                class_type_params,
                shadowed,
            );
        }
    }
    rewrite_class_alias_value(
        element,
        class_path,
        class_locals,
        class_type_params,
        shadowed,
    );
    for name in added {
        shadowed.remove(&name);
    }
}
