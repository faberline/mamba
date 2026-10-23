//! Enum inheritance follows each inferred direct base and original transform bounds.
use super::{classes::Class, enum_members, resolution::Context, Checker};
use mamba_language::application::quality_syntax::python_ast::StmtClassDef;
use ruff_text_size::Ranged;
use std::rc::Rc;

impl Checker<'_> {
    pub(super) fn enum_subtype(&self, class: Rc<Class>, origin: Context, name: &str) -> bool {
        self.mro(class, origin).is_ok_and(|order| {
            order
                .iter()
                .any(|(class, _)| class.identity.0 == "enum" && class.name == name)
        })
    }

    pub(super) fn enum_member(&self, class: Rc<Class>, origin: Context, name: &str) -> bool {
        class.identity.0 != "enum"
            && self.enum_subtype(class.clone(), origin, "Enum")
            && self
                .source_class(&class)
                .is_some_and(|source| enum_members::collect(&source.body).get(name) == Some(&true))
    }

    pub(super) fn check_enum_base(
        &mut self,
        statement: &StmtClassDef,
        ancestor: Rc<Class>,
        origin: Context,
    ) {
        if !self.enum_subtype(ancestor.clone(), origin.clone(), "Enum") {
            return;
        }
        if ancestor.identity.0 != "enum"
            && self.source_class(&ancestor).is_some_and(|source| {
                enum_members::collect(&source.body)
                    .values()
                    .any(|value| *value)
            })
        {
            self.emit(
                "E0244",
                "invalid-enum-extension",
                &format!("Extending inherited Enum class \"{}\"", ancestor.name),
                self.declaration_range(statement.name.range()),
            );
        }
        if self.enum_subtype(ancestor, origin, "IntFlag") {
            self.check_enum_flags(statement);
        }
    }
}
