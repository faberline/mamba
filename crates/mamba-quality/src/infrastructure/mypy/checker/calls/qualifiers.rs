//! Pinned model Final diagnostics use the declaration's complete range.
use super::*;
use quality_syntax::python_ast::StmtClassDef;
mod initializers;
pub(in crate::infrastructure::mypy::checker) mod literals;

impl Checker<'_> {
    pub(super) fn check_model_qualifiers(&mut self, class: &StmtClassDef) {
        let variables = self.scopes.variables();
        let mut initialized = initializers::collect(class);
        for statement in
            super::super::super::classes::statements::collect(&class.body, self.python_target)
        {
            let Stmt::AnnAssign(assignment) = statement else {
                continue;
            };
            if pydantic::qualifiers::form(&assignment.annotation, &self.context, &variables)
                .as_deref()
                != Some("Final")
            {
                continue;
            }
            let Some(value) = assignment.value.as_deref() else {
                let name = if let Expr::Name(name) = assignment.target.as_ref() {
                    name.id.as_str()
                } else {
                    ""
                };
                if let Some(offset) = initialized.remove(name) {
                    self.final_initializers.insert(offset);
                } else {
                    self.emit(
                        "misc",
                        "Final name must be initialized with a value".into(),
                        statement,
                    );
                }
                continue;
            };
            if !matches!(assignment.annotation.as_ref(), Expr::Subscript(_))
                && !literals::simple(value, &self.final_constants)
            {
                self.emit(
                    "misc",
                    "Need type argument for Final[...] with non-literal default in BaseModel"
                        .into(),
                    statement,
                );
            }
        }
    }

    pub(super) fn inherited_model_class_var(&self, name: &str) -> bool {
        self.class_types
            .last()
            .copied()
            .flatten()
            .and_then(|ty| self.context.class_id(ty))
            .is_some_and(|id| {
                self.context
                    .class_lineage(id)
                    .into_iter()
                    .skip(1)
                    .any(|parent| {
                        let class = self.context.class(parent);
                        class.pydantic.as_ref().is_some_and(|model| {
                            class.fields.get(name).is_some_and(|field| {
                                field.class_var
                                    || (field.final_attribute
                                        && model.fields.iter().all(|field| field.name != name))
                            })
                        })
                    })
            })
    }
}
