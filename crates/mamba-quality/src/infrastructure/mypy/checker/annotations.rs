//! Unparameterized Callable retains strict's original annotation diagnostic.
use super::*;
mod qualifiers;

impl Checker<'_> {
    pub(super) fn check_callable_annotation(&mut self, expression: &Expr) {
        if let Some(ty) = self.context.returned_callable_validation_name(expression) {
            let variables = self.scopes.annotation_variables();
            let (previous_variables, previous_bound) = self
                .scopes
                .annotation_scope(variables, std::collections::BTreeSet::new());
            let mut bound = previous_bound.clone();
            bound.insert(ty);
            self.scopes
                .annotation_scope(previous_variables.clone(), bound);
            self.check_callable_annotation_inner(expression);
            self.scopes
                .annotation_scope(previous_variables, previous_bound);
        } else {
            self.check_callable_annotation_inner(expression);
        }
    }

    fn check_callable_annotation_inner(&mut self, expression: &Expr) {
        if self.classes.is_empty()
            && self
                .context
                .typing_form(expression, &self.scopes.annotation_variables())
                .as_deref()
                == Some("Self")
        {
            self.emit(
                "misc",
                "Self type is only allowed in annotations within class definition".into(),
                expression,
            );
            return;
        }
        if let Some(message) = self
            .context
            .predicate_annotation_error(expression, &self.scopes.variables())
        {
            self.emit("valid-type", message, expression);
            return;
        }
        if self.check_annotated_qualifier(expression) {
            return;
        }
        if self.check_async_annotation(expression) {
            return;
        }
        if self.check_type_object_annotation(expression) {
            return;
        }
        if self.check_tuple_variable_annotation(expression) {
            return;
        }
        if self.check_parameter_spec_use(expression)
            || self.check_parameter_spec_callable(expression)
        {
            return;
        }
        if self.check_alias_annotation(expression) {
            return;
        }
        if self.check_builtin_annotation(expression) {
            return;
        }
        if self.check_class_annotation(expression) {
            return;
        }
        if self.check_named_annotation(expression) {
            return;
        }
        if let Expr::Subscript(subscript) = expression {
            if let Some(message) = self
                .context
                .literal_annotation_error(subscript, &self.scopes.variables())
            {
                self.emit("valid-type", message, expression);
                return;
            }
        }
        let bare = match expression {
            Expr::Name(name) => name.id.as_str() == "Callable",
            Expr::Attribute(attribute) => attribute.attr.as_str() == "Callable",
            _ => false,
        };
        if bare
            && self
                .scopes
                .annotation(expression)
                .and_then(|ty| self.context.node(ty))
                .is_some_and(|node| matches!(node.as_ref(), types::Node::Callable(_)))
        {
            self.emit(
                "type-arg",
                "Missing type arguments for generic type \"Callable\"".into(),
                expression,
            );
            return;
        }
        if let Expr::Subscript(subscript) = expression {
            if self.scopes.annotation(expression).is_none()
                && self.scopes.annotation(&subscript.value).is_none()
                && matches!(subscript.value.as_ref(), Expr::Name(name) if self.scopes.has_binding(name.id.as_str()))
                && self.check_named_annotation(&subscript.value)
            {
                return;
            }
        }
        match expression {
            Expr::Subscript(subscript) => self.check_callable_annotation(&subscript.slice),
            Expr::Tuple(tuple) => {
                for item in &tuple.elts {
                    self.check_callable_annotation(item);
                }
            }
            Expr::List(list) => {
                for item in &list.elts {
                    self.check_callable_annotation(item);
                }
            }
            Expr::BinOp(binary) => {
                self.check_callable_annotation(&binary.left);
                self.check_callable_annotation(&binary.right);
            }
            _ => {}
        }
    }
}
