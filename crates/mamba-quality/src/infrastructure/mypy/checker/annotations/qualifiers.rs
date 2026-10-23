//! Annotated metadata does not contribute a type argument.
use super::*;

impl Checker<'_> {
    pub(super) fn check_annotated_qualifier(&mut self, expression: &Expr) -> bool {
        let Expr::Subscript(subscript) = expression else {
            return false;
        };
        let variables = self.scopes.variables();
        if self
            .context
            .typing_form(&subscript.value, &variables)
            .as_deref()
            != Some("Annotated")
        {
            return false;
        }
        let first = if let Expr::Tuple(tuple) = subscript.slice.as_ref() {
            tuple.elts.first()
        } else {
            Some(subscript.slice.as_ref())
        };
        if let Some(first) = first {
            if pydantic::qualifiers::form(first, &self.context, &variables).as_deref()
                == Some("ClassVar")
            {
                self.emit(
                    "valid-type",
                    "Invalid type: ClassVar nested inside other type".into(),
                    first,
                );
            } else {
                self.check_callable_annotation(first);
            }
        }
        true
    }
}
