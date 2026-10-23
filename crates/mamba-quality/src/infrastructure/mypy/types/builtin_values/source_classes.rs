//! Project-bound builtin views keep the cached native declaration graph neutral.
use super::*;

impl Context {
    pub(in crate::infrastructure::mypy) fn register_embedded_source_module(&self, namespace: &str) {
        let Some((project, module)) = namespace.split_once(':') else {
            return;
        };
        let target = slot(self.module_target(namespace));
        let mut registry = self.classes.borrow_mut();
        let declarations: Vec<_> = registry
            .entries
            .iter()
            .enumerate()
            .filter_map(|(id, class)| {
                let nested = class.namespace.strip_prefix(namespace)?;
                if !nested.is_empty() && !nested.starts_with('.') {
                    return None;
                }
                Some((
                    format!("{module}{nested}.{}", class.name),
                    u32::try_from(id).expect("source class ID fits in u32"),
                ))
            })
            .collect();
        for (name, id) in declarations {
            if registry
                .embedded_source_classes
                .insert((project.into(), target, name), id)
                != Some(id)
            {
                registry.native_source_solutions.clear();
            }
        }
    }

    pub(in crate::infrastructure::mypy) fn embedded_source_class_name(
        &self,
        id: u32,
    ) -> Option<String> {
        let class = self.class(id);
        let (project, module) = class.namespace.split_once(':')?;
        let name = format!("{module}.{}", class.name);
        let registry = self.classes.borrow();
        let target = slot(*registry.module_targets.get(&class.namespace)?);
        let key = (project.into(), target, name.clone());
        (registry.embedded_source_classes.get(&key) == Some(&id)).then_some(name)
    }

    pub(in crate::infrastructure::mypy) fn native_source_value(
        &self,
        value: Type,
        namespace: &str,
    ) -> Type {
        let Some((project, _)) = namespace.split_once(':') else {
            return value;
        };
        let target = slot(self.module_target(namespace));
        let key = (project.to_owned(), target);
        let cached = self
            .classes
            .borrow()
            .native_source_solutions
            .get(&key)
            .cloned();
        let solutions = cached.unwrap_or_else(|| {
            let solutions: BTreeMap<Type, Type> = {
                let registry = self.classes.borrow();
                registry
                    .embedded_source_classes
                    .iter()
                    .filter(|((owner, version, _), _)| owner == project && *version == target)
                    .filter_map(|((_, _, name), &source)| {
                        // Reader.named and locked class predeclaration share this key.
                        let native = *registry.keys.get(&(name.clone(), 0))?;
                        let native = &registry.entries[native as usize];
                        let source = &registry.entries[source as usize];
                        Some([
                            (native.instance, source.instance),
                            (native.object, source.object),
                        ])
                    })
                    .flatten()
                    .collect()
            };
            let solutions = std::sync::Arc::new(solutions);
            self.classes
                .borrow_mut()
                .native_source_solutions
                .insert(key, solutions.clone());
            solutions
        });
        if solutions.is_empty() {
            return value;
        }
        // Existing substitution visits Callable/overloads, Union and containers.
        // It does not change cached values, class members or class bases.
        self.substitute(value, &solutions)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_source_map_tracks_late_declarations_registration_and_identity_changes() {
        use mamba_language::application::quality_syntax::PythonTarget;

        let context = Context::default();
        context.set_module_target("0:source".into(), PythonTarget::Py312);
        let source_id = context.declare_class("0:source", "Foo", 10);
        context.register_embedded_source_module("0:source");
        assert_eq!(
            context.native_source_value(Type::Str, "0:source"),
            Type::Str
        );

        // A late native declaration must replace an earlier empty map.
        let native_id = context.declare_class("source.Foo", "Foo", 0);
        let native = context.class(native_id);
        let source = context.class(source_id);
        assert_eq!(
            context.native_source_value(native.object, "0:source"),
            source.object
        );

        // Metadata refresh must keep a reusable map when type identity is stable.
        let cached = context
            .classes
            .borrow()
            .native_source_solutions
            .values()
            .next()
            .unwrap()
            .clone();
        let mut refreshed = (*source).clone();
        refreshed.final_class = true;
        context.replace_class(source_id, refreshed);
        assert_eq!(
            context.native_source_value(native.instance, "0:source"),
            source.instance
        );
        assert!(std::sync::Arc::ptr_eq(
            &cached,
            context
                .classes
                .borrow()
                .native_source_solutions
                .values()
                .next()
                .unwrap()
        ));

        let spare_id = context.declare_class("replacement", "Replacement", 1);
        let spare = context.class(spare_id);
        assert_eq!(
            context.native_source_value(native.instance, "0:source"),
            source.instance
        );
        let mut replaced = (*context.class(source_id)).clone();
        replaced.instance = spare.instance;
        context.replace_class(source_id, replaced);
        assert_eq!(
            context.native_source_value(native.instance, "0:source"),
            spare.instance
        );
        let mut replaced = (*context.class(source_id)).clone();
        replaced.object = spare.object;
        context.replace_class(source_id, replaced);
        assert_eq!(
            context.native_source_value(native.object, "0:source"),
            spare.object
        );

        // Re-registration can replace an existing key without changing map length.
        let next_id = context.declare_class("0:source", "Foo", 20);
        let next = context.class(next_id);
        assert_eq!(
            context.native_source_value(native.object, "0:source"),
            spare.object
        );
        context.register_embedded_source_module("0:source");
        assert_eq!(
            context.native_source_value(native.object, "0:source"),
            next.object
        );

        // Native identity changes must remove old keys and install new keys.
        let mut replaced = (*native).clone();
        replaced.instance = spare.instance;
        context.replace_class(native_id, replaced);
        assert_eq!(
            context.native_source_value(native.instance, "0:source"),
            native.instance
        );
        assert_eq!(
            context.native_source_value(spare.instance, "0:source"),
            next.instance
        );
        let mut replaced = (*context.class(native_id)).clone();
        replaced.object = spare.object;
        context.replace_class(native_id, replaced);
        assert_eq!(
            context.native_source_value(native.object, "0:source"),
            native.object
        );
        assert_eq!(
            context.native_source_value(spare.object, "0:source"),
            next.object
        );
    }
}
