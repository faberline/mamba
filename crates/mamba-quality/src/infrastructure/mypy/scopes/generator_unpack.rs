//! Synchronous generator tuple unpacking retains the inferred element type.
use super::*;

impl Scopes {
    pub(super) fn generator_assignment_bindings<'a>(
        &self,
        targets: &'a [Expr],
        generator: Type,
    ) -> Option<Vec<(&'a Expr, Type)>> {
        if targets.is_empty() {
            return None;
        }
        let item = self.context.iterable_item(generator)?;
        let mut bindings = Vec::with_capacity(targets.len());
        let mut starred_seen = false;
        for target in targets {
            match target {
                Expr::Name(_) => bindings.push((target, item)),
                Expr::Starred(starred)
                    if !starred_seen && matches!(starred.value.as_ref(), Expr::Name(_)) =>
                {
                    starred_seen = true;
                    let rest = self
                        .context
                        .intern(types::Node::Container(types::Container::List, vec![item]));
                    bindings.push((starred.value.as_ref(), rest));
                }
                // Nested/attribute targets remain outside this captured slice.
                _ => return None,
            }
        }
        Some(bindings)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mamba_language::application::quality_syntax::{self, PythonTarget};
    use std::rc::Rc;

    #[test]
    fn generator_leaves_keep_item_and_star_list_types() {
        let context = Rc::new(types::Context::default());
        let scopes = Scopes::new(&[], context.clone(), "0:probe".into());
        let syntax = quality_syntax::parse(
            "first, *middle, last = source\n",
            PythonTarget::Py312,
            false,
        )
        .unwrap();
        let Stmt::Assign(assignment) = &syntax.parsed.syntax().body[0] else {
            panic!("fixture assign")
        };
        for item in [Type::Int, Type::Any] {
            let generator = context.intern(types::Node::Container(
                types::Container::Generator,
                vec![item, Type::None, Type::None],
            ));
            assert!(scopes
                .generator_assignment_bindings(&[], generator)
                .is_none());
            let bindings = scopes
                .assignment_bindings(&assignment.targets[0], generator)
                .unwrap();
            assert_eq!(bindings.len(), 3);
            assert_eq!(bindings[0].1, item);
            assert_eq!(bindings[2].1, item);
            assert!(matches!(context.node(bindings[1].1).as_deref(),
                Some(types::Node::Container(types::Container::List, items)) if items == &[item]));
        }
    }

    #[test]
    fn unobserved_iterable_async_and_list_target_paths_stay_unchanged() {
        let context = Rc::new(types::Context::default());
        let scopes = Scopes::new(&[], context.clone(), "0:probe".into());
        let tuple =
            quality_syntax::parse("left, right = source\n", PythonTarget::Py312, false).unwrap();
        let Stmt::Assign(assignment) = &tuple.parsed.syntax().body[0] else {
            panic!("fixture assign")
        };
        for (kind, arguments) in [
            (types::Container::Iterable, vec![Type::Int]),
            (
                types::Container::AsyncGenerator,
                vec![Type::Int, Type::None],
            ),
        ] {
            let ty = context.intern(types::Node::Container(kind, arguments));
            assert!(scopes
                .assignment_bindings(&assignment.targets[0], ty)
                .is_none());
        }
        let list =
            quality_syntax::parse("[left, right] = source\n", PythonTarget::Py312, false).unwrap();
        let Stmt::Assign(assignment) = &list.parsed.syntax().body[0] else {
            panic!("fixture assign")
        };
        let generator = context.intern(types::Node::Container(
            types::Container::Generator,
            vec![Type::Int, Type::None, Type::None],
        ));
        assert!(scopes
            .assignment_bindings(&assignment.targets[0], generator)
            .is_none());
    }
}
