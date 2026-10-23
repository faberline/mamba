//! A model field can only be replaced by another model field.
use super::*;
use quality_syntax::python_ast::StmtClassDef;

impl Checker<'_> {
    pub(in crate::infrastructure::mypy::checker) fn check_model_field_overrides(
        &mut self,
        class: &StmtClassDef,
        model: &pydantic::Model,
    ) {
        let mut arguments = std::collections::BTreeSet::new();
        for field in model.init_fields() {
            let name = field.argument_name(!model.uses_aliases());
            if !arguments.insert(name) {
                self.emit_column_zero(
                    "no-redef",
                    format!("Name \"{name}\" already defined (possibly by an import)"),
                    class,
                );
            }
        }
        for statement in &class.body {
            let (name, range) = match statement {
                Stmt::FunctionDef(function) => (function.name.as_str(), function.range()),
                Stmt::ClassDef(nested) => (nested.name.as_str(), nested.range()),
                _ => continue,
            };
            if model.fields.iter().any(|field| field.name == name) {
                self.emit(
                    "misc",
                    "BaseModel field may only be overridden by another field".into(),
                    &range,
                );
            }
        }
    }
}
