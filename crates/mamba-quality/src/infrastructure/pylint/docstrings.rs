//! Empty documentation uses declaration locations rather than literal locations.
use super::Checker;
use mamba_language::application::quality_syntax::python_ast::{Expr, Stmt};
use ruff_text_size::TextRange;

impl Checker<'_> {
    pub(super) fn assigned_class_docstring(&self, body: &[Stmt]) -> bool {
        let (_, first) =
            super::callables::collect_with_first(body, self.module_name, self.is_package);
        let Some(super::callables::Target::Value(value)) = first.get("__doc__") else {
            return false;
        };
        let value = self.expand_value(
            (**value).clone(),
            &super::resolution::Context::Scope(self.scopes.current_index()),
            &mut Default::default(),
        );
        matches!(
            value,
            super::values::Value::String(_)
                | super::values::Value::Bytes(_)
                | super::values::Value::Number(_)
                | super::values::Value::Boolean(_)
                | super::values::Value::Scalar("NoneType")
        )
    }

    pub(super) fn check_empty_docstring(&mut self, body: &[Stmt], kind: &str, range: TextRange) {
        if let Some(Stmt::Expr(statement)) = body.first() {
            if let Expr::StringLiteral(string) = statement.value.as_ref() {
                if string.value.to_str().trim().is_empty() {
                    self.emit(
                        "C0112",
                        "empty-docstring",
                        &format!("Empty {kind} docstring"),
                        range,
                    );
                }
            }
        }
    }
}
