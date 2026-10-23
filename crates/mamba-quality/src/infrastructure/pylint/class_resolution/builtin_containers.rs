//! Builtin class containers use the original runtime hierarchy, not stub bases.
use super::super::{classes::Class, resolution::Context, Checker};
use std::rc::Rc;

impl Checker<'_> {
    pub(in super::super) fn builtin_class_container(
        &self,
        class: &Class,
        name: &str,
    ) -> Option<Vec<(Rc<Class>, Context)>> {
        if class.identity.0 != "builtins" {
            return None;
        }
        let model = super::builtin_scope::class(&class.name, self.modules.target())?;
        let names = if name == "__bases__" {
            &model.bases
        } else {
            &model.mro_names
        };
        names
            .iter()
            .map(|name| {
                let name = name.strip_prefix("builtins.")?;
                Some((
                    self.modules.builtin_class(name)?,
                    Context::Module("builtins".into()),
                ))
            })
            .collect()
    }
}
