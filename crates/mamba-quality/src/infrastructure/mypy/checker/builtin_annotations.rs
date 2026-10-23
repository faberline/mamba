//! Imported container annotations retain strict arity errors and their type.
use super::*;

impl Checker<'_> {
    pub(super) fn check_builtin_annotation(&mut self, expression: &Expr) -> bool {
        let (base, arguments) = match expression {
            Expr::Subscript(value) => {
                let arguments = match value.slice.as_ref() {
                    Expr::Tuple(tuple) => tuple.elts.iter().collect::<Vec<_>>(),
                    value => vec![value],
                };
                (value.value.as_ref(), Some(arguments))
            }
            Expr::Name(_) | Expr::Attribute(_) => (expression, None),
            _ => return false,
        };
        let Some(form) = self
            .context
            .typing_form(base, &self.scopes.annotation_variables())
        else {
            return false;
        };
        let Some(name) = form.strip_prefix("builtin:") else {
            return false;
        };
        if !matches!(name, "list" | "dict" | "set" | "frozenset" | "tuple") {
            return false;
        }
        let Some(arguments) = arguments else {
            let display = if name == "tuple" {
                annotation_name(base)
            } else {
                name.into()
            };
            let name = display.as_str();
            self.emit(
                "type-arg",
                format!("Missing type arguments for generic type \"{name}\""),
                expression,
            );
            return true;
        };
        if name == "tuple" {
            return false;
        }
        let count = if name == "dict" { 2 } else { 1 };
        if arguments.len() == count {
            return false;
        }
        let given = if arguments.is_empty() {
            "none".into()
        } else {
            arguments.len().to_string()
        };
        self.emit(
            "type-arg",
            format!(
                "\"{name}\" expects {count} type argument{}, but {given} given",
                if count == 1 { "" } else { "s" }
            ),
            expression,
        );
        if arguments.is_empty() {
            self.emit(
                "type-arg",
                format!("Missing type arguments for generic type \"{name}\""),
                expression,
            );
        }
        for argument in arguments {
            self.check_callable_annotation(argument);
        }
        true
    }
}

fn annotation_name(expression: &Expr) -> String {
    match expression {
        Expr::Name(name) => name.id.to_string(),
        Expr::Attribute(attribute) => {
            format!("{}.{}", annotation_name(&attribute.value), attribute.attr)
        }
        _ => "tuple".into(),
    }
}
