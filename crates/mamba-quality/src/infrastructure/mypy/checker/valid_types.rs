//! Unknown annotation names and unbound variables keep their original spans.
use super::*;

impl Checker<'_> {
    pub(super) fn check_named_annotation(&mut self, expression: &Expr) -> bool {
        let Expr::Name(name) = expression else {
            return false;
        };
        if self
            .context
            .typing_form(expression, &self.scopes.variables())
            .is_some()
            || self.scopes.imported_dynamic_type(name.id.as_str())
        {
            return false;
        }
        if let Some(ty) = self.scopes.annotation(expression) {
            if self
                .context
                .node(ty)
                .is_some_and(|node| matches!(node.as_ref(), types::Node::Variable(variable) if !variable.parameter_spec))
                && !self.scopes.type_variable_bound(ty)
                && !self.class_types.iter().flatten().any(|&class| {
                    self.context
                        .class_id(class)
                        .is_some_and(|id| self.context.class(id).generic.contains(&ty))
                })
            {
                self.emit_with_hint("valid-type", format!("Type variable \"{}\" is unbound", self.scopes.qualified_name(name.id.as_str())), &ruff_text_size::TextRange::at(expression.start(), ruff_text_size::TextSize::new(1)), Some("(Hint: Use \"Generic[T]\" or \"Protocol[T]\" base class to bind \"T\" inside a class)\n(Hint: Use \"T\" in function signature to bind \"T\" inside a function)".into()));
                return true;
            }
            return false;
        }
        if self.scopes.has_binding(name.id.as_str()) {
            self.emit_with_hint("valid-type", format!("Variable \"{}\" is not valid as a type", self.scopes.qualified_name(name.id.as_str())), &ruff_text_size::TextRange::at(expression.start(), ruff_text_size::TextSize::new(1)), Some("See https://mypy.readthedocs.io/en/stable/common_issues.html#variables-vs-type-aliases".into()));
        } else {
            self.emit(
                "name-defined",
                format!("Name \"{}\" is not defined", name.id),
                expression,
            );
        }
        true
    }
}
